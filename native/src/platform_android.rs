//! Android backend: ALVR client core, MediaCodec decoder, EGL/GLES renderer.
//!
//! Threads:
//! * ALVR's own connection/decode threads (owned by `alvr_client_core`).
//! * Event thread: drains ALVR events and configures the decoder.
//! * Input thread: sends tracking and (deduplicated) view parameters.
//! * Render thread: the Kotlin `RenderThread` that calls `nativeRenderFrame`;
//!   it owns the EGL context and the GL renderer (kept in thread-local state).
//!
//! This module is only compiled for Android. Host tests exercise the same
//! policy through `runtime`.

use crate::{lock_runtime, DisplayInfo};
use alvr_client_core::{
    video_decoder::{self, VideoDecoderConfig, VideoDecoderSource},
    ClientCapabilities, ClientCoreContext, ClientCoreEvent,
};
use alvr_common::{
    glam::{Quat as AlvrQuat, UVec2, Vec3 as AlvrVec3},
    DeviceMotion, Fov as AlvrFov, Pose, ViewParams as AlvrViewParams, HEAD_ID,
};
use alvr_packets::FaceData;
use alvr_session::{MediacodecPropType, MediacodecProperty};
use glow::HasContext;
use jni::objects::JObject;
use jni::JNIEnv;
use phonealvr_core::{Action, Quat, Settings};
use phonealvr_render::egl::EglContext;
use phonealvr_render::texture::create_external_texture;
use phonealvr_render::{ExternalTexture, FrameParams, StereoRenderer, TextureKind};
use phonealvr_viewer::{build_eye_mesh, EyeMapper};
use std::{
    cell::RefCell,
    ffi::c_void,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const MESH_COLUMNS: u32 = 40;
const MESH_ROWS: u32 = 32;

struct AlvrState {
    context: Arc<ClientCoreContext>,
    decoder: Arc<Mutex<Option<VideoDecoderSource>>>,
    running: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

static ALVR: OnceLock<Mutex<Option<AlvrState>>> = OnceLock::new();

static BRIGHTNESS_BITS: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0f32
static VIGNETTE_BITS: AtomicU32 = AtomicU32::new(0);
static CA_BITS: AtomicU32 = AtomicU32::new(0);
static MESH_DIRTY: AtomicBool = AtomicBool::new(true);
static ANDROID_CONTEXT_SET: AtomicBool = AtomicBool::new(false);
static SURFACE_WIDTH: AtomicU32 = AtomicU32::new(1);
static SURFACE_HEIGHT: AtomicU32 = AtomicU32::new(1);
static POSE_X: AtomicU32 = AtomicU32::new(0);
static POSE_Y: AtomicU32 = AtomicU32::new(0);
static POSE_Z: AtomicU32 = AtomicU32::new(0);
static POSE_W: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0f32
static STREAM_VIEW_WIDTH: AtomicU32 = AtomicU32::new(0);
static STREAM_VIEW_HEIGHT: AtomicU32 = AtomicU32::new(0);
static FIRST_FRAME_LOGGED: AtomicBool = AtomicBool::new(false);

fn alvr_slot() -> &'static Mutex<Option<AlvrState>> {
    ALVR.get_or_init(|| Mutex::new(None))
}

fn alvr_lock() -> std::sync::MutexGuard<'static, Option<AlvrState>> {
    alvr_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn f32_from_atomic(value: &AtomicU32) -> f32 {
    f32::from_bits(value.load(Ordering::Relaxed))
}

fn store_f32(value: &AtomicU32, number: f32) {
    value.store(number.to_bits(), Ordering::Relaxed);
}

pub fn set_android_context(env: &mut JNIEnv, context: &JObject) {
    if let Ok(vm) = env.get_java_vm() {
        unsafe {
            ndk_context::initialize_android_context(
                vm.get_java_vm_pointer().cast(),
                context.as_raw().cast(),
            );
        }
        ANDROID_CONTEXT_SET.store(true, Ordering::Release);
    }
}

pub fn initialize(display: DisplayInfo) -> Result<(), String> {
    let level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    let _ = android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(level)
            .with_tag("PhoneALVR"),
    );
    alvr_client_core::init_logging();

    let per_eye = {
        let guard = lock_runtime();
        guard
            .as_ref()
            .map(|runtime| runtime.effective_view_resolution())
            .unwrap_or_else(|| display.native_per_eye())
    };

    let capabilities = ClientCapabilities {
        default_view_resolution: UVec2::new(per_eye.width, per_eye.height),
        refresh_rates: vec![display.refresh_hz],
        foveated_encoding: false,
        encoder_high_profile: true,
        // The v1 renderer samples the decoder texture as 8-bit SDR; do not
        // negotiate 10-bit/HDR formats it cannot colour-manage yet.
        encoder_10_bits: false,
        encoder_av1: true,
        prefer_10bit: false,
        prefer_full_range: false,
        preferred_encoding_gamma: 1.0,
        prefer_hdr: false,
    };

    let context = Arc::new(ClientCoreContext::new(capabilities));
    context.resume();

    let running = Arc::new(AtomicBool::new(true));
    let decoder: Arc<Mutex<Option<VideoDecoderSource>>> = Arc::new(Mutex::new(None));

    let event_thread = spawn_event_thread(
        Arc::clone(&context),
        Arc::clone(&decoder),
        Arc::clone(&running),
    );
    let input_thread = spawn_input_thread(Arc::clone(&context), Arc::clone(&running));

    *alvr_lock() = Some(AlvrState {
        context,
        decoder,
        running,
        threads: vec![event_thread, input_thread],
    });
    MESH_DIRTY.store(true, Ordering::Relaxed);
    Ok(())
}

pub fn handle_actions(actions: &[Action]) {
    let guard = alvr_lock();
    let Some(state) = guard.as_ref() else {
        return;
    };
    for action in actions {
        match action {
            Action::StartListener => state.context.resume(),
            Action::StopListener => state.context.pause(),
            Action::StopStream => {
                *state
                    .decoder
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
            }
            Action::ReportError(message) => log::error!("session error: {message}"),
            Action::StateChanged(session_state) => {
                log::info!("session state: {session_state}");
            }
            Action::AwaitHandshake | Action::PauseRender | Action::ResumeRender => {}
        }
    }
}

pub fn shutdown() {
    if let Some(state) = alvr_lock().take() {
        state.running.store(false, Ordering::SeqCst);
        for thread in state.threads {
            let _ = thread.join();
        }
        *state
            .decoder
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        drop(state.context);
    }
    if ANDROID_CONTEXT_SET.swap(false, Ordering::AcqRel) {
        unsafe { ndk_context::release_android_context() };
    }
}

pub fn apply_settings(json: &str) {
    if let Ok(settings) = Settings::from_json(json) {
        store_f32(&BRIGHTNESS_BITS, settings.brightness);
        store_f32(&VIGNETTE_BITS, settings.vignette);
        store_f32(&CA_BITS, settings.chromatic_aberration);
    }
}

pub fn apply_viewer_profile() {
    MESH_DIRTY.store(true, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// GL / EGL render thread state
// ---------------------------------------------------------------------------

struct RenderState {
    egl: EglContext,
    gl: glow::Context,
    renderer: StereoRenderer,
    window: *mut c_void,
    width: i32,
    height: i32,
    tan_half_horizontal: f32,
    tan_half_vertical: f32,
    stream_texture: Option<glow::Texture>,
}

thread_local! {
    static RENDER: RefCell<Option<RenderState>> = const { RefCell::new(None) };
}

#[link(name = "android")]
extern "C" {
    fn ANativeWindow_fromSurface(
        env: *mut jni::sys::JNIEnv,
        surface: jni::sys::jobject,
    ) -> *mut c_void;
    fn ANativeWindow_release(window: *mut c_void);
}

pub fn surface_created(env: &mut JNIEnv, surface: &JObject) -> bool {
    let window = unsafe { ANativeWindow_fromSurface(env.get_raw(), surface.as_raw()) };
    if window.is_null() {
        log::error!("ANativeWindow_fromSurface returned null");
        return false;
    }

    let egl = match unsafe { EglContext::create(window) } {
        Ok(egl) => egl,
        Err(error) => {
            log::error!("EGL initialization failed: {error}");
            unsafe { ANativeWindow_release(window) };
            return false;
        }
    };

    let gl = unsafe { glow::Context::from_loader_function(|name| EglContext::glow_loader(name)) };
    let mut renderer = match StereoRenderer::new(&gl) {
        Ok(renderer) => renderer,
        Err(error) => {
            log::error!("renderer initialization failed: {error}");
            drop(egl);
            unsafe { ANativeWindow_release(window) };
            return false;
        }
    };

    let profile = lock_runtime()
        .as_ref()
        .map(|runtime| runtime.profile().clone())
        .unwrap_or_default();
    let mapper = EyeMapper::from_profile(&profile);
    let mesh = build_eye_mesh(&mapper, MESH_COLUMNS, MESH_ROWS);
    if let Err(error) = renderer.upload_mesh(&gl, &mesh) {
        log::error!("mesh upload failed: {error}");
    }
    let tan = profile.fov.tan_extents();
    let stream_texture = create_external_texture(&gl);
    MESH_DIRTY.store(false, Ordering::Relaxed);

    log::info!(
        "surface created: EGL {}x{}, renderer ready",
        SURFACE_WIDTH.load(Ordering::Acquire),
        SURFACE_HEIGHT.load(Ordering::Acquire)
    );
    RENDER.with(|slot| {
        *slot.borrow_mut() = Some(RenderState {
            egl,
            gl,
            renderer,
            window,
            width: 1,
            height: 1,
            tan_half_horizontal: (tan.right - tan.left) * 0.5,
            tan_half_vertical: (tan.top - tan.bottom) * 0.5,
            stream_texture,
        });
    });

    let actions = lock_runtime()
        .as_mut()
        .map(|runtime| runtime.on_surface_available())
        .unwrap_or_default();
    handle_actions(&actions);
    true
}

pub fn surface_changed(width: i32, height: i32) {
    // This is called from the UI thread; the render thread owns the EGL
    // context and applies the new size at the start of the next frame.
    SURFACE_WIDTH.store(width.max(1) as u32, Ordering::Release);
    SURFACE_HEIGHT.store(height.max(1) as u32, Ordering::Release);
}

pub fn surface_destroyed() {
    RENDER.with(|slot| {
        if let Some(mut state) = slot.borrow_mut().take() {
            let _ = state.egl.make_current();
            FIRST_FRAME_LOGGED.store(false, Ordering::Relaxed);
            unsafe {
                state.renderer.destroy(&state.gl);
                if let Some(texture) = state.stream_texture.take() {
                    state.gl.delete_texture(texture);
                }
                ANativeWindow_release(state.window);
            }
        }
    });
}

pub fn render_frame() {
    RENDER.with(|slot| {
        let mut borrow = slot.borrow_mut();
        let Some(state) = borrow.as_mut() else {
            return;
        };
        if state.egl.make_current().is_err() {
            return;
        }

        let requested_width = SURFACE_WIDTH.load(Ordering::Acquire) as i32;
        let requested_height = SURFACE_HEIGHT.load(Ordering::Acquire) as i32;
        if requested_width != state.width || requested_height != state.height {
            state.width = requested_width.max(1);
            state.height = requested_height.max(1);
            state.renderer.resize(state.width, state.height);
        }

        if MESH_DIRTY.swap(false, Ordering::Relaxed) {
            let profile = lock_runtime()
                .as_ref()
                .map(|runtime| runtime.profile().clone())
                .unwrap_or_default();
            let mapper = EyeMapper::from_profile(&profile);
            let mesh = build_eye_mesh(&mapper, MESH_COLUMNS, MESH_ROWS);
            if let Err(error) = state.renderer.upload_mesh(&state.gl, &mesh) {
                log::error!("mesh upload failed: {error}");
            }
            let tan = profile.fov.tan_extents();
            state.tan_half_horizontal = (tan.right - tan.left) * 0.5;
            state.tan_half_vertical = (tan.top - tan.bottom) * 0.5;
        }

        let (frame, frame_orientation) = {
            let guard = alvr_lock();
            match guard.as_ref() {
                Some(alvr) => {
                    let mut source = alvr
                        .decoder
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    match source.as_mut().and_then(|source| source.get_frame()) {
                        Some((timestamp, buffer)) => {
                            // Report compositor start before drawing so ALVR
                            // can return the head pose it used for this frame.
                            let views = alvr.context.report_compositor_start(timestamp);
                            (Some((timestamp, buffer)), Some(views[0].pose.orientation))
                        }
                        None => (None, None),
                    }
                }
                None => (None, None),
            }
        };

        let Some((timestamp, buffer_ptr)) = frame else {
            unsafe {
                state.gl.clear_color(0.0, 0.0, 0.0, 1.0);
                state.gl.clear(glow::COLOR_BUFFER_BIT);
            }
            state.egl.swap_buffers();
            return;
        };

        let mut params = FrameParams {
            brightness: f32_from_atomic(&BRIGHTNESS_BITS),
            vignette: f32_from_atomic(&VIGNETTE_BITS),
            chromatic_aberration: f32_from_atomic(&CA_BITS),
            reprojection: [0.0, 0.0],
            background: [0.0, 0.0, 0.0, 1.0],
        };

        if let Some(frame_orientation) = frame_orientation {
            let current = Quat::new(
                f32_from_atomic(&POSE_X),
                f32_from_atomic(&POSE_Y),
                f32_from_atomic(&POSE_Z),
                f32_from_atomic(&POSE_W),
            );
            let frame_pose = Quat::new(
                frame_orientation.x,
                frame_orientation.y,
                frame_orientation.z,
                frame_orientation.w,
            );
            params.reprojection = phonealvr_core::reprojection_offset(
                current,
                frame_pose,
                state.tan_half_horizontal,
                state.tan_half_vertical,
            );
        }

        let external = match state.stream_texture {
            Some(texture) => ExternalTexture::from_hardware_buffer_with_texture(
                &state.gl,
                state.egl.display(),
                buffer_ptr,
                texture,
            ),
            None => {
                ExternalTexture::from_hardware_buffer(&state.gl, state.egl.display(), buffer_ptr)
            }
        };
        let mut external = match external {
            Ok(texture) => texture,
            Err(error) => {
                log::warn!("frame texture failed: {error}");
                return;
            }
        };

        if !FIRST_FRAME_LOGGED.swap(true, Ordering::Relaxed) {
            log::info!(
                "first decoded frame: {}x{} surface, reprojection {:?}",
                state.width,
                state.height,
                params.reprojection
            );
        }
        let texture = external.texture();
        let render_result = unsafe {
            state
                .renderer
                .render(&state.gl, [texture, texture], TextureKind::External, params)
        };
        external.destroy(&state.gl);
        if let Err(error) = render_result {
            log::warn!("render failed: {error}");
            return;
        }

        state.egl.set_presentation_time(boot_time_ns() + 11_000_000);
        state.egl.swap_buffers();

        if let Some(alvr) = alvr_lock().as_ref() {
            alvr.context.report_submit(timestamp, Duration::ZERO);
        }
        let now_ns = boot_time_ns();
        let frame_ns = timestamp.as_nanos().min(u64::MAX as u128) as u64;
        let latency_ms = now_ns.saturating_sub(frame_ns) as f32 / 1_000_000.0;
        if let Some(runtime) = lock_runtime().as_mut() {
            let stats = runtime.stats_mut();
            stats.on_frame(now_ns);
            stats.on_latency(latency_ms);
        }
    });
}

pub fn on_orientation(orientation: Quat) {
    store_f32(&POSE_X, orientation.x);
    store_f32(&POSE_Y, orientation.y);
    store_f32(&POSE_Z, orientation.z);
    store_f32(&POSE_W, orientation.w);
}

// ---------------------------------------------------------------------------
// Worker threads
// ---------------------------------------------------------------------------

fn spawn_event_thread(
    context: Arc<ClientCoreContext>,
    decoder: Arc<Mutex<Option<VideoDecoderSource>>>,
    running: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("phonealvr-events".into())
        .spawn(move || {
            while running.load(Ordering::Relaxed) {
                while let Some(event) = context.poll_event() {
                    handle_event(&context, &decoder, event);
                }
                thread::sleep(Duration::from_millis(5));
            }
        })
        .expect("failed to spawn event thread")
}

fn handle_event(
    context: &Arc<ClientCoreContext>,
    decoder: &Arc<Mutex<Option<VideoDecoderSource>>>,
    event: ClientCoreEvent,
) {
    match event {
        ClientCoreEvent::StreamingStarted(stream) => {
            let width = stream.negotiated_config.view_resolution.x;
            let height = stream.negotiated_config.view_resolution.y;
            let refresh = stream.negotiated_config.refresh_rate_hint;
            log::info!("streaming started: {width}x{height} @ {refresh} Hz");
            STREAM_VIEW_WIDTH.store(width, Ordering::Release);
            STREAM_VIEW_HEIGHT.store(height, Ordering::Release);
            let bitrate = bitrate_mbps_from_settings(&stream.settings);
            if let Some(runtime) = lock_runtime().as_mut() {
                let native = runtime.display().native_per_eye();
                runtime.on_handshake_succeeded(20);
                runtime.on_stream_started(width, height, refresh, 20);
                if width > native.width || height > native.height {
                    let warning = format!(
                        "Stream {width}x{height} per eye exceeds the panel's {native_width}x{native_height}. Set ALVR Video → Resolution to Scale 1.0 or lower.",
                        native_width = native.width,
                        native_height = native.height,
                    );
                    log::warn!("{warning}");
                    runtime.on_hud_message(warning);
                }
                if let Some(bitrate) = bitrate {
                    runtime.stats_mut().set_bitrate_mbps(bitrate);
                }
            }
        }
        ClientCoreEvent::StreamingStopped => {
            log::info!("streaming stopped");
            STREAM_VIEW_WIDTH.store(0, Ordering::Release);
            STREAM_VIEW_HEIGHT.store(0, Ordering::Release);
            *decoder
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
            if let Some(runtime) = lock_runtime().as_mut() {
                runtime.on_stream_stopped();
            }
        }
        ClientCoreEvent::DecoderConfig { codec, config_nal } => {
            let codec_name = format!("{codec:?}").to_lowercase();
            let view_width = STREAM_VIEW_WIDTH.load(Ordering::Acquire);
            let view_height = STREAM_VIEW_HEIGHT.load(Ordering::Acquire);
            let profile = h264_profile_name(&config_nal);
            if matches!(profile, "high10" | "high422" | "high444") {
                if let Some(runtime) = lock_runtime().as_mut() {
                    runtime.on_error(format!(
                        "unsupported H.264 profile {profile}; disable 10-bit/HDR in ALVR"
                    ));
                }
            }
            log::info!(
                "decoder config: {codec_name}, {} bytes, H.264 profile {}, csd {:02x?}, view {}x{} (encoded {}x{})",
                config_nal.len(),
                h264_profile_name(&config_nal),
                config_nal,
                view_width,
                view_height,
                view_width * 2,
                view_height
            );

            // ALVR's decoder uses 512x1024 placeholders and relies on the codec
            // reading the SPS. Some MediaTek/Qualcomm decoders need the real
            // encoded size; the options are applied after the placeholders.
            let mut options = Vec::new();
            if view_width > 0 && view_height > 0 {
                options.push((
                    "width".to_owned(),
                    MediacodecProperty {
                        ty: MediacodecPropType::Int32,
                        value: (view_width * 2).to_string(),
                    },
                ));
                options.push((
                    "height".to_owned(),
                    MediacodecProperty {
                        ty: MediacodecPropType::Int32,
                        value: view_height.to_string(),
                    },
                ));
            }

            let config = VideoDecoderConfig {
                codec,
                force_software_decoder: false,
                max_buffering_frames: 2.0,
                buffering_history_weight: 0.9,
                options,
                config_buffer: config_nal,
            };

            let context_for_callback = Arc::clone(context);
            let (mut sink, source) =
                video_decoder::create_decoder(config, move |result| match result {
                    Ok(timestamp) => context_for_callback.report_frame_decoded(timestamp),
                    Err(error) => {
                        context_for_callback.report_fatal_decoder_error(&error.to_string())
                    }
                });

            context.set_decoder_input_callback(Box::new(move |timestamp, buffer| {
                sink.push_nal(timestamp, buffer)
            }));

            *decoder
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(source);
            if let Some(runtime) = lock_runtime().as_mut() {
                runtime.stats_mut().set_codec(&codec_name);
            }
        }
        ClientCoreEvent::UpdateHudMessage(message) => {
            log::info!("ALVR: {message}");
            if let Some(runtime) = lock_runtime().as_mut() {
                runtime.on_hud_message(message);
            }
        }
        ClientCoreEvent::Haptics { .. } => {}
        ClientCoreEvent::RealTimeConfig(_) => {}
    }
}

fn spawn_input_thread(context: Arc<ClientCoreContext>, running: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("phonealvr-input".into())
        .spawn(move || {
            while running.load(Ordering::Relaxed) {
                let timestamp_ns = boot_time_ns();
                let (pose, angular_velocity, view_params, streaming) = {
                    let mut guard = lock_runtime();
                    match guard.as_mut() {
                        Some(runtime) => (
                            runtime.predicted_pose(timestamp_ns),
                            runtime.tracker().angular_velocity(),
                            runtime.take_view_params_if_changed(),
                            runtime.is_streaming(),
                        ),
                        None => (Quat::IDENTITY, phonealvr_core::Vec3::ZERO, None, false),
                    }
                };

                if streaming {
                    if let Some(view_params) = view_params {
                        context.send_view_params([
                            to_alvr_view(view_params.0[0]),
                            to_alvr_view(view_params.0[1]),
                        ]);
                    }

                    let motion = DeviceMotion {
                        pose: Pose {
                            orientation: AlvrQuat::from_xyzw(pose.x, pose.y, pose.z, pose.w),
                            position: AlvrVec3::ZERO,
                        },
                        linear_velocity: AlvrVec3::ZERO,
                        angular_velocity: AlvrVec3::new(
                            angular_velocity.x,
                            angular_velocity.y,
                            angular_velocity.z,
                        ),
                    };
                    context.send_tracking(
                        Duration::from_nanos(timestamp_ns),
                        vec![(*HEAD_ID, motion)],
                        [None, None],
                        FaceData::default(),
                    );
                }

                thread::sleep(Duration::from_millis(5));
            }
        })
        .expect("failed to spawn input thread")
}

fn h264_profile_name(nal: &[u8]) -> &'static str {
    nal.iter()
        .position(|byte| *byte == 0x67)
        .and_then(|index| nal.get(index + 1))
        .map(|profile| match *profile {
            66 => "baseline",
            77 => "main",
            88 => "extended",
            100 => "high",
            110 => "high10",
            122 => "high422",
            244 => "high444",
            _ => "unknown",
        })
        .unwrap_or("not-h264")
}

fn bitrate_mbps_from_settings(settings: &alvr_session::Settings) -> Option<f32> {
    let value = serde_json::to_value(settings).ok()?;
    let mode = value.get("video")?.get("bitrate")?.get("mode")?;
    if let Some(number) = mode.as_f64() {
        return Some(number as f32);
    }
    let object = mode.as_object()?;
    object
        .get("ConstantMbps")
        .and_then(|value| value.as_f64())
        .or_else(|| {
            object
                .get("Adaptive")
                .and_then(|value| value.get("max_throughput_mbps"))
                .and_then(|value| value.as_f64())
        })
        .map(|value| value as f32)
}

fn to_alvr_view(eye: phonealvr_core::EyeView) -> AlvrViewParams {
    AlvrViewParams {
        pose: Pose {
            orientation: AlvrQuat::IDENTITY,
            position: AlvrVec3::new(eye.position[0], eye.position[1], eye.position[2]),
        },
        fov: AlvrFov {
            left: eye.fov.left,
            right: eye.fov.right,
            up: eye.fov.up,
            down: eye.fov.down,
        },
    }
}

fn boot_time_ns() -> u64 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let result = unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut time) };
    if result == 0 {
        (time.tv_sec as u64) * 1_000_000_000 + time.tv_nsec as u64
    } else {
        0
    }
}
