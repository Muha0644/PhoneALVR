//! Platform-independent controller used by the JNI layer and by host tests.
//!
//! The Android backend owns sockets, ALVR, EGL and MediaCodec. This type owns
//! the policy: session transitions, settings, tracking prediction, view-param
//! deduplication and statistics. Keeping it separate makes the risky parts
//! testable without a device or an ALVR server.

use phonealvr_core::{
    Action, Event, Fov, HeadTracker, ImuSample, Quat, Resolution, Session, SessionState, Settings,
    StatsTracker, ViewParamDedup, ViewParams,
};
use phonealvr_viewer::{EyeFov, FoveatedEncoding, ViewerProfile};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: f32,
    /// Physical horizontal DPI of the panel, used to convert the viewer's
    /// millimetre lens spacing into screen coordinates.
    pub xdpi: f32,
}

impl DisplayInfo {
    pub const fn new(width: u32, height: u32, refresh_hz: f32) -> Self {
        Self {
            width,
            height,
            refresh_hz,
            xdpi: 400.0,
        }
    }

    pub const fn with_dpi(width: u32, height: u32, refresh_hz: f32, xdpi: f32) -> Self {
        Self {
            width,
            height,
            refresh_hz,
            xdpi,
        }
    }

    /// Long dimension of the panel in metres (landscape width).
    pub fn physical_width_m(self) -> f32 {
        let dpi = if self.xdpi.is_finite() && self.xdpi > 50.0 {
            self.xdpi
        } else {
            400.0
        };
        self.width.max(self.height) as f32 / dpi * 0.0254
    }

    /// Side-by-side layout: each eye gets half of the long side and all of the
    /// short side, in physical pixels.
    pub fn native_per_eye(self) -> Resolution {
        Resolution::new(self.width.max(self.height) / 2, self.width.min(self.height))
    }
}

pub struct Runtime {
    session: Session,
    settings: Settings,
    profile: ViewerProfile,
    tracker: HeadTracker,
    stats: StatsTracker,
    dedup: ViewParamDedup,
    display: DisplayInfo,
    view_params: ViewParams,
    foveated_encoding: Option<FoveatedEncoding>,
    first_tracking_sample: bool,
    last_error: Option<String>,
    hud_message: Option<String>,
}

impl Runtime {
    pub fn new(display: DisplayInfo) -> Self {
        let settings = Settings::default();
        let profile = ViewerProfile::cardboard_v1();
        let view_params = view_params_for(&profile, 0.0);
        Self {
            session: Session::new(),
            settings,
            profile,
            tracker: HeadTracker::new(),
            stats: StatsTracker::default(),
            dedup: ViewParamDedup::default(),
            display,
            view_params,
            foveated_encoding: None,
            first_tracking_sample: true,
            last_error: None,
            hud_message: None,
        }
    }

    pub fn start(&mut self) -> Vec<Action> {
        self.first_tracking_sample = true;
        self.session.handle(Event::Start)
    }

    pub fn stop(&mut self) -> Vec<Action> {
        let actions = self.session.handle(Event::Stop);
        self.dedup.invalidate();
        self.stats.reset_session_counters();
        actions
    }

    pub fn destroy(&mut self) -> Vec<Action> {
        let actions = self.session.handle(Event::Close);
        self.dedup.invalidate();
        actions
    }

    pub fn reset(&mut self) -> Vec<Action> {
        self.session.handle(Event::Reset)
    }

    pub fn on_server_connected(&mut self, peer: impl Into<String>) -> Vec<Action> {
        self.session
            .handle(Event::ServerConnected { peer: peer.into() })
    }

    pub fn on_handshake_started(&mut self) -> Vec<Action> {
        self.session.handle(Event::HandshakeStarted)
    }

    pub fn on_handshake_succeeded(&mut self, protocol: u32) -> Vec<Action> {
        self.session.handle(Event::HandshakeSucceeded { protocol })
    }

    pub fn on_handshake_failed(&mut self, reason: impl Into<String>) -> Vec<Action> {
        self.session.handle(Event::HandshakeFailed {
            reason: reason.into(),
        })
    }

    pub fn on_stream_started(
        &mut self,
        width: u32,
        height: u32,
        refresh_hz: f32,
        protocol: u32,
    ) -> Vec<Action> {
        self.dedup.invalidate();
        self.stats.set_stream(width, height, "unknown");
        self.session.handle(Event::StreamStarted {
            view_width: width,
            view_height: height,
            refresh_hz,
            protocol,
        })
    }

    pub fn on_stream_stopped(&mut self) -> Vec<Action> {
        self.dedup.invalidate();
        self.foveated_encoding = None;
        self.session.handle(Event::StreamStopped)
    }

    pub fn on_network_lost(&mut self) -> Vec<Action> {
        self.session.handle(Event::NetworkLost)
    }

    pub fn on_network_restored(&mut self) -> Vec<Action> {
        self.session.handle(Event::NetworkRestored)
    }

    pub fn on_surface_available(&mut self) -> Vec<Action> {
        self.session.handle(Event::SurfaceAvailable)
    }

    pub fn on_surface_lost(&mut self) -> Vec<Action> {
        self.session.handle(Event::SurfaceLost)
    }

    pub fn on_hud_message(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.hud_message = if message.trim().is_empty() {
            None
        } else {
            Some(message)
        };
    }

    pub fn on_error(&mut self, message: impl Into<String>) -> Vec<Action> {
        let message = message.into();
        self.last_error = Some(message.clone());
        self.session.handle(Event::Error(message))
    }

    pub fn on_tracking(&mut self, sample: ImuSample) {
        self.tracker.ingest(sample);
        // The absolute rotation-vector frame is arbitrary. Treat the pose at
        // the first sample of a session as forward so the user does not start
        // looking at the floor; an explicit recenter remains available.
        if self.first_tracking_sample {
            self.tracker.recenter();
            self.first_tracking_sample = false;
        }
        self.stats.on_tracking(sample.timestamp_ns);
    }

    pub fn predicted_pose(&self, target_timestamp_ns: u64) -> Quat {
        self.tracker.pose_at(target_timestamp_ns)
    }

    pub fn tracker(&self) -> &HeadTracker {
        &self.tracker
    }

    pub fn recenter(&mut self) {
        self.tracker.recenter();
        self.first_tracking_sample = false;
    }

    pub fn set_settings_json(&mut self, json: &str) -> Result<(), String> {
        let settings = Settings::from_json(json).map_err(|error| error.to_string())?;

        let profile = match &settings.viewer_profile {
            Some(profile_json) => {
                let profile = ViewerProfile::parse(profile_json)
                    .map_err(|error| format!("viewer profile: {error}"))?;
                profile
            }
            None => ViewerProfile::cardboard_v1(),
        };

        self.settings = settings;
        self.profile = profile;
        self.view_params = view_params_for(&self.profile, self.settings.ipd_mm);
        self.dedup.invalidate();
        Ok(())
    }

    /// Feed a profile already parsed by the Kotlin QR scanner or file picker.
    pub fn set_viewer_profile(&mut self, profile: ViewerProfile) -> Result<(), String> {
        let profile = profile.validate().map_err(|error| error.to_string())?;
        self.profile = profile;
        self.view_params = view_params_for(&self.profile, self.settings.ipd_mm);
        self.dedup.invalidate();
        Ok(())
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn profile(&self) -> &ViewerProfile {
        &self.profile
    }

    pub fn display(&self) -> DisplayInfo {
        self.display
    }

    /// Record the negotiated foveated-encoding layout for the current stream,
    /// or clear it when the stream is not foveated. The layout carries the
    /// smaller encoded per-eye size used by the decoder and the UV mapping
    /// constants used by the renderer.
    pub fn set_foveated_encoding(&mut self, encoding: Option<FoveatedEncoding>) {
        self.foveated_encoding = encoding;
    }

    pub fn foveated_encoding(&self) -> Option<FoveatedEncoding> {
        self.foveated_encoding
    }

    pub fn effective_view_resolution(&self) -> Resolution {
        self.settings
            .effective_view_resolution(self.display.native_per_eye())
    }

    pub fn view_params(&self) -> ViewParams {
        self.view_params
    }

    /// Returns true exactly when the platform should call ALVR's
    /// `send_view_params`. Never call that function at tracking rate.
    pub fn take_view_params_if_changed(&mut self) -> Option<ViewParams> {
        if self.dedup.should_send(self.view_params) {
            Some(self.view_params)
        } else {
            None
        }
    }

    pub fn state(&self) -> SessionState {
        self.session.state()
    }

    pub fn is_streaming(&self) -> bool {
        self.session.state().is_streaming()
    }

    pub fn status_json(&self) -> String {
        let mut value: serde_json::Value =
            serde_json::from_str(&self.session.status_json()).unwrap_or_default();
        value["hud_message"] = self
            .hud_message
            .as_deref()
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null);
        if let Some(encoding) = self.foveated_encoding {
            if let Some(stream) = value
                .get_mut("stream")
                .and_then(|value| value.as_object_mut())
            {
                let [width, height] = encoding.encoded_view_resolution();
                stream.insert("foveated".into(), true.into());
                stream.insert("encoded_view_width".into(), width.into());
                stream.insert("encoded_view_height".into(), height.into());
            }
        }
        value.to_string()
    }

    pub fn stats_json(&self) -> String {
        self.stats.to_json()
    }

    pub fn stats_mut(&mut self) -> &mut StatsTracker {
        &mut self.stats
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error
            .as_deref()
            .or_else(|| self.session.last_error())
    }

    pub fn diagnostics_json(&self) -> String {
        serde_json::json!({
            "session": self.session.status(),
            "display": {
                "width": self.display.width,
                "height": self.display.height,
                "refresh_hz": self.display.refresh_hz,
                "native_per_eye": {
                    "width": self.display.native_per_eye().width,
                    "height": self.display.native_per_eye().height,
                },
            },
            "settings": self.settings.diagnostics_json(),
            "stats": self.stats.snapshot(),
            "viewer": {
                "vendor": self.profile.vendor,
                "model": self.profile.model,
                "ipd_m": self.profile.inter_lens_distance,
                "screen_to_lens_m": self.profile.screen_to_lens_distance,
            },
        })
        .to_string()
    }
}

/// `ipd_override_mm` is 0 to use the viewer profile. Lens spacing and human
/// IPD can differ, and convergence is sensitive to the difference.
pub fn view_params_for(profile: &ViewerProfile, ipd_override_mm: f32) -> ViewParams {
    let ipd = if ipd_override_mm.is_finite() && ipd_override_mm > 0.0 {
        ipd_override_mm / 1000.0
    } else {
        profile.inter_lens_distance
    };
    let half_ipd = ipd * 0.5;
    let fov = alvr_fov(profile.fov);
    ViewParams([
        phonealvr_core::EyeView {
            position: [-half_ipd, 0.0, 0.0],
            fov,
        },
        phonealvr_core::EyeView {
            position: [half_ipd, 0.0, 0.0],
            fov,
        },
    ])
}

fn alvr_fov(fov: EyeFov) -> Fov {
    Fov::new(
        -fov.left.to_radians(),
        fov.right.to_radians(),
        fov.top.to_radians(),
        -fov.bottom.to_radians(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonealvr_core::Quat;

    fn runtime() -> Runtime {
        Runtime::new(DisplayInfo::new(2400, 1080, 90.0))
    }

    #[test]
    fn native_per_eye_is_long_side_over_two_by_short_side() {
        assert_eq!(
            DisplayInfo::new(2400, 1080, 90.0).native_per_eye(),
            Resolution::new(1200, 1080)
        );
        assert_eq!(
            DisplayInfo::new(1080, 2400, 90.0).native_per_eye(),
            Resolution::new(1200, 1080)
        );
    }

    #[test]
    fn physical_width_uses_long_side_and_dpi() {
        let display = DisplayInfo::with_dpi(2400, 1080, 60.0, 400.0);
        assert!((display.physical_width_m() - 0.1524).abs() < 1e-4);
    }

    #[test]
    fn effective_resolution_is_aligned_down() {
        let runtime = runtime();
        assert_eq!(
            runtime.effective_view_resolution(),
            Resolution::new(1184, 1056)
        );
    }

    #[test]
    fn lifecycle_reaches_streaming() {
        let mut runtime = runtime();
        runtime.start();
        runtime.on_server_connected("192.0.2.2:50000");
        runtime.on_handshake_started();
        runtime.on_handshake_succeeded(20);
        runtime.on_stream_started(1184, 1056, 90.0, 20);
        assert!(runtime.is_streaming());
        assert!(runtime.status_json().contains("\"streaming\""));
    }

    #[test]
    fn foveated_encoding_is_stream_scoped() {
        let mut runtime = runtime();
        runtime.start();
        runtime.on_server_connected("p");
        runtime.on_handshake_started();
        runtime.on_stream_started(1184, 1056, 90.0, 20);
        assert!(runtime.foveated_encoding().is_none());

        runtime.set_foveated_encoding(Some(FoveatedEncoding::new(
            [1184, 1056],
            phonealvr_viewer::FoveatedConfig::default(),
        )));
        let encoding = runtime.foveated_encoding().expect("foveated encoding");
        assert_eq!(encoding.base_resolution, [1184, 1056]);
        assert!(encoding.encoded_resolution[0] < 1184);
        assert!(encoding.encoded_resolution[1] < 1056);

        let status = runtime.status_json();
        assert!(status.contains("\"foveated\":true"));
        assert!(status.contains(&format!(
            "\"encoded_view_width\":{}",
            encoding.encoded_resolution[0]
        )));

        runtime.on_stream_stopped();
        assert!(runtime.foveated_encoding().is_none());
    }

    #[test]
    fn ipd_override_changes_eye_separation() {
        let mut runtime = runtime();
        let settings = Settings {
            ipd_mm: 64.0,
            ..Settings::default()
        };
        runtime.set_settings_json(&settings.to_json()).unwrap();
        let params = runtime.view_params();
        let separation = params.0[1].position[0] - params.0[0].position[0];
        assert!((separation - 0.064).abs() < 1e-6, "{separation}");
    }

    #[test]
    fn view_params_are_not_spammed() {
        let mut runtime = runtime();
        runtime.start();
        runtime.on_server_connected("p");
        runtime.on_handshake_started();
        runtime.on_stream_started(1184, 1056, 90.0, 20);
        assert!(runtime.take_view_params_if_changed().is_some());
        for _ in 0..1000 {
            assert!(runtime.take_view_params_if_changed().is_none());
        }
    }

    #[test]
    fn changing_ipd_recomputes_view_params() {
        let mut runtime = runtime();
        let before = runtime.view_params();
        let json = serde_json::json!({
            "viewer_profile": r#"{
                "interLensDistance": 0.070,
                "screenToLensDistance": 0.042,
                "distortionCoefficients": [0.441, 0.156]
            }"#
        })
        .to_string();
        runtime.set_settings_json(&json).unwrap();
        let after = runtime.view_params();
        assert!((after.0[0].position[0] - (-0.035)).abs() < 1e-6);
        assert!((before.0[0].position[0] - (-0.030)).abs() < 1e-6);
        assert!(runtime.take_view_params_if_changed().is_some());
    }

    #[test]
    fn clearing_viewer_profile_resets_to_builtin() {
        let mut runtime = runtime();
        let custom = Settings {
            viewer_profile: Some(
                r#"{"interLensDistance":0.070,"screenToLensDistance":0.042}"#.to_owned(),
            ),
            ..Settings::default()
        };
        runtime.set_settings_json(&custom.to_json()).unwrap();
        assert!((runtime.profile().inter_lens_distance - 0.070).abs() < 1e-6);
        runtime
            .set_settings_json(&Settings::default().to_json())
            .unwrap();
        assert!((runtime.profile().inter_lens_distance - 0.060).abs() < 1e-6);
    }

    #[test]
    fn invalid_viewer_profile_is_rejected_without_losing_state() {
        let mut runtime = runtime();
        let before = runtime.profile().clone();
        let json = r#"{"viewer_profile":"{\"interLensDistance\": 999}"}"#;
        assert!(runtime.set_settings_json(json).is_err());
        assert_eq!(runtime.profile(), &before);
    }

    #[test]
    fn settings_json_round_trip() {
        let mut runtime = runtime();
        let settings = Settings {
            brightness: 0.5,
            debug_overlay: true,
            ..Settings::default()
        };
        runtime.set_settings_json(&settings.to_json()).unwrap();
        assert_eq!(runtime.settings().brightness, 0.5);
        assert!(runtime.settings().debug_overlay);
    }

    #[test]
    fn hud_message_is_included_in_status() {
        let mut runtime = runtime();
        runtime.on_hud_message("Searching for streamer...");
        assert!(runtime.status_json().contains("Searching for streamer"));
        runtime.on_hud_message("");
        assert!(runtime.status_json().contains("\"hud_message\":null"));
    }

    #[test]
    fn first_tracking_sample_defines_forward() {
        let mut runtime = runtime();
        runtime.start();
        runtime.on_tracking(ImuSample {
            orientation: Quat::from_axis_angle(phonealvr_core::Vec3::new(0.0, 1.0, 0.0), 1.2),
            timestamp_ns: 10,
        });
        let pose = runtime.predicted_pose(10);
        assert!(pose.angle_to(Quat::IDENTITY) < 1e-5);
    }

    #[test]
    fn recenter_zeroes_prediction() {
        let mut runtime = runtime();
        runtime.on_tracking(ImuSample {
            orientation: Quat::from_axis_angle(phonealvr_core::Vec3::new(0.0, 1.0, 0.0), 1.0),
            timestamp_ns: 1,
        });
        runtime.recenter();
        let pose = runtime.predicted_pose(1);
        assert!(pose.angle_to(Quat::IDENTITY) < 1e-5);
    }

    #[test]
    fn stop_and_destroy_are_safe_in_any_order() {
        let mut runtime = runtime();
        runtime.stop();
        runtime.destroy();
        runtime.destroy();
        runtime.stop();
        assert_eq!(runtime.state(), SessionState::Closed);
    }

    #[test]
    fn one_hundred_cycles_are_stable() {
        let mut runtime = runtime();
        for _ in 0..100 {
            runtime.start();
            runtime.on_server_connected("p");
            runtime.on_handshake_started();
            runtime.on_stream_started(1184, 1056, 90.0, 20);
            runtime.on_stream_stopped();
            runtime.stop();
            assert_eq!(runtime.state(), SessionState::Idle);
        }
    }

    #[test]
    fn diagnostics_redacts_crash_endpoint() {
        let mut runtime = runtime();
        let json = r#"{
            "crash_reporting": true,
            "crash_reporting_endpoint": "https://example.invalid/report"
        }"#;
        runtime.set_settings_json(json).unwrap();
        let diagnostics = runtime.diagnostics_json();
        assert!(!diagnostics.contains("example.invalid"));
        assert!(diagnostics.contains("<configured>"));
    }
}
