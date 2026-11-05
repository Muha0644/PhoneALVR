//! JNI translation layer. One function per Kotlin `external fun`; everything
//! here is deliberately boring.

use crate::{lock_runtime, platform, DisplayInfo, Runtime};
use jni::objects::{GlobalRef, JClass, JObject, JString};
use jni::sys::{jboolean, jfloat, jint, jlong, JNI_FALSE, JNI_TRUE, JNI_VERSION_1_6};
use jni::{JNIEnv, JavaVM};
use phonealvr_core::{Event, ImuSample, Quat};
use phonealvr_viewer::ViewerProfile;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

static ANDROID_CONTEXT: OnceLock<Mutex<Option<GlobalRef>>> = OnceLock::new();

fn android_context_slot() -> &'static Mutex<Option<GlobalRef>> {
    ANDROID_CONTEXT.get_or_init(|| Mutex::new(None))
}

#[no_mangle]
pub extern "system" fn JNI_OnLoad(_vm: JavaVM, _reserved: *mut c_void) -> jint {
    JNI_VERSION_1_6
}

fn java_string(env: &mut JNIEnv, value: &JString) -> String {
    env.get_string(value)
        .map(|value| value.into())
        .unwrap_or_default()
}

fn jstring(env: &mut JNIEnv, value: &str) -> jni::sys::jstring {
    env.new_string(value)
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn runtime_mut<R>(default: R, function: impl FnOnce(&mut Runtime) -> R) -> R {
    let mut guard = lock_runtime();
    match guard.as_mut() {
        Some(runtime) => function(runtime),
        None => default,
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeCreate(
    mut env: JNIEnv,
    _class: JClass,
    width: jint,
    height: jint,
    refresh_hz: jfloat,
    xdpi: jfloat,
    android_context: JObject,
    settings_json: JString,
) -> jboolean {
    let width = width.max(1) as u32;
    let height = height.max(1) as u32;
    let refresh_hz = if refresh_hz.is_finite() && refresh_hz > 0.0 {
        refresh_hz
    } else {
        60.0
    };
    let xdpi = if xdpi.is_finite() && xdpi > 50.0 {
        xdpi
    } else {
        400.0
    };
    let display = DisplayInfo::with_dpi(width, height, refresh_hz, xdpi);

    // ndk_context keeps the raw jobject for the lifetime of the session.
    // A local reference would be invalid after this call, so promote it to a
    // global reference and release it in nativeDestroy.
    let global_context = match env.new_global_ref(&android_context) {
        Ok(context) => context,
        Err(_) => return JNI_FALSE,
    };
    platform::set_android_context(&mut env, global_context.as_obj());
    *android_context_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(global_context);
    let settings_json = java_string(&mut env, &settings_json);
    // Create the policy object first: the Android backend reads the effective
    // per-eye resolution from it when advertising client capabilities.
    {
        let mut guard = lock_runtime();
        let mut runtime = Runtime::new(display);
        if let Err(error) = runtime.set_settings_json(&settings_json) {
            log::warn!("initial settings rejected: {error}");
        }
        *guard = Some(runtime);
    }
    if platform::initialize(display).is_err() {
        platform::shutdown();
        *lock_runtime() = None;
        return JNI_FALSE;
    }
    JNI_TRUE
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeDestroy(
    _env: JNIEnv,
    _class: JClass,
) {
    let actions = runtime_mut(Vec::new(), |runtime| runtime.destroy());
    platform::handle_actions(&actions);
    platform::shutdown();
    *android_context_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    let mut guard = lock_runtime();
    *guard = None;
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeStart(
    _env: JNIEnv,
    _class: JClass,
) {
    let actions = runtime_mut(Vec::new(), |runtime| runtime.start());
    platform::handle_actions(&actions);
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeStop(
    _env: JNIEnv,
    _class: JClass,
) {
    let actions = runtime_mut(Vec::new(), |runtime| runtime.stop());
    platform::handle_actions(&actions);
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeUpdateSettings(
    mut env: JNIEnv,
    _class: JClass,
    json: JString,
) -> jboolean {
    let json = java_string(&mut env, &json);
    let result = runtime_mut(Err("runtime is not created".to_owned()), |runtime| {
        runtime.set_settings_json(&json)
    });
    match result {
        Ok(()) => {
            platform::apply_settings(&json);
            // Settings can carry a new viewer profile; rebuild the mesh.
            platform::apply_viewer_profile();
            JNI_TRUE
        }
        Err(error) => {
            log::warn!("settings rejected: {error}");
            JNI_FALSE
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeSetViewerProfile(
    mut env: JNIEnv,
    _class: JClass,
    json: JString,
) -> jboolean {
    let json = java_string(&mut env, &json);
    let profile = match ViewerProfile::parse(&json) {
        Ok(profile) => profile,
        Err(error) => {
            log::warn!("viewer profile rejected: {error}");
            return JNI_FALSE;
        }
    };
    let result = runtime_mut(Err("runtime is not created".to_owned()), |runtime| {
        runtime.set_viewer_profile(profile)
    });
    match result {
        Ok(()) => {
            platform::apply_viewer_profile();
            JNI_TRUE
        }
        Err(error) => {
            log::warn!("viewer profile rejected: {error}");
            JNI_FALSE
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeOnSurfaceCreated(
    mut env: JNIEnv,
    _class: JClass,
    surface: JObject,
) -> jboolean {
    if platform::surface_created(&mut env, &surface) {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeOnSurfaceChanged(
    _env: JNIEnv,
    _class: JClass,
    width: jint,
    height: jint,
) {
    platform::surface_changed(width.max(1), height.max(1));
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeOnSurfaceDestroyed(
    _env: JNIEnv,
    _class: JClass,
) {
    platform::surface_destroyed();
    runtime_mut((), |runtime| {
        runtime.on_surface_lost();
    });
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeRenderFrame(
    _env: JNIEnv,
    _class: JClass,
) {
    platform::render_frame();
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeRecenter(
    _env: JNIEnv,
    _class: JClass,
) {
    runtime_mut((), |runtime| runtime.recenter());
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativePushOrientation(
    _env: JNIEnv,
    _class: JClass,
    x: jfloat,
    y: jfloat,
    z: jfloat,
    w: jfloat,
    timestamp_ns: jlong,
    display_rotation: jint,
) {
    if !(x.is_finite() && y.is_finite() && z.is_finite() && w.is_finite()) {
        return;
    }
    let orientation = phonealvr_core::android_orientation_to_openvr(
        Quat::new(x, y, z, w),
        display_rotation.clamp(0, 3) as u8,
    );
    let sample = ImuSample {
        orientation,
        timestamp_ns: timestamp_ns.max(0) as u64,
    };
    let pose = runtime_mut(None, |runtime| {
        runtime.on_tracking(sample);
        Some(runtime.predicted_pose(timestamp_ns.max(0) as u64))
    });
    if let Some(pose) = pose {
        platform::on_orientation(pose);
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativePollStatus(
    mut env: JNIEnv,
    _class: JClass,
) -> jni::sys::jstring {
    let status = runtime_mut("{}".to_owned(), |runtime| runtime.status_json());
    jstring(&mut env, &status)
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeGetStats(
    mut env: JNIEnv,
    _class: JClass,
) -> jni::sys::jstring {
    let dropped = platform::video_drop_count();
    let stats = runtime_mut("{}".to_owned(), |runtime| {
        runtime.stats_mut().set_dropped_frames(dropped);
        runtime.stats_json()
    });
    jstring(&mut env, &stats)
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeGetDiagnostics(
    mut env: JNIEnv,
    _class: JClass,
) -> jni::sys::jstring {
    let diagnostics = runtime_mut("{}".to_owned(), |runtime| runtime.diagnostics_json());
    jstring(&mut env, &diagnostics)
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeOnServerConnected(
    mut env: JNIEnv,
    _class: JClass,
    peer: JString,
) {
    let peer = java_string(&mut env, &peer);
    let actions = runtime_mut(Vec::new(), |runtime| runtime.on_server_connected(peer));
    platform::handle_actions(&actions);
}

#[no_mangle]
pub extern "system" fn Java_dev_phonealvr_client_NativeSession_nativeOnError(
    mut env: JNIEnv,
    _class: JClass,
    message: JString,
) {
    let message = java_string(&mut env, &message);
    runtime_mut((), |runtime| {
        let actions = runtime.on_error(message);
        platform::handle_actions(&actions);
    });
}

/// Test hook used by the host integration tests; not called from Kotlin.
#[allow(dead_code)]
pub fn inject_event(event: Event) {
    runtime_mut((), |runtime| match event {
        Event::Start => {
            runtime.start();
        }
        Event::Stop => {
            runtime.stop();
        }
        Event::Close => {
            runtime.destroy();
        }
        Event::ServerConnected { peer } => {
            runtime.on_server_connected(peer);
        }
        Event::StreamStopped => {
            runtime.on_stream_stopped();
        }
        Event::NetworkLost => {
            runtime.on_network_lost();
        }
        Event::NetworkRestored => {
            runtime.on_network_restored();
        }
        other => {
            let _ = other;
        }
    });
}
