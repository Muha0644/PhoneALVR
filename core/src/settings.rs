//! Persisted user settings and their validation rules.
//!
//! Settings arrive from Kotlin as JSON so the native core and the UI can evolve
//! independently. Validation is intentionally centralised here: malformed or
//! hostile values must never reach the renderer or the ALVR capabilities.

use serde::{Deserialize, Serialize};

pub const MAX_VIEWER_PROFILE_BYTES: usize = 64 * 1024;
/// ALVR aligns resolutions down to a multiple of 32 pixels.
pub const ALVR_RESOLUTION_ALIGNMENT: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Largest resolution not exceeding this one that ALVR will encode.
    pub fn aligned_down(self) -> Self {
        Self {
            width: (self.width / ALVR_RESOLUTION_ALIGNMENT) * ALVR_RESOLUTION_ALIGNMENT,
            height: (self.height / ALVR_RESOLUTION_ALIGNMENT) * ALVR_RESOLUTION_ALIGNMENT,
        }
    }

    pub fn is_zero(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn min(self, other: Self) -> Self {
        Self {
            width: self.width.min(other.width),
            height: self.height.min(other.height),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Informational hint only. PhoneALVR never dials the streamer.
    pub server: Option<String>,
    /// Cardboard-compatible viewer profile JSON (or a decoded QR payload).
    pub viewer_profile: Option<String>,
    /// `None` means "follow the panel / streamer".
    pub refresh_rate_hz: Option<f32>,
    /// Per-eye cap. `None` or zero means "native panel".
    pub resolution_cap: Option<Resolution>,
    /// 0.0..=1.0. The platform layer may also apply this to the window.
    pub brightness: f32,
    /// Optional lens vignette compensation, 0.0..=1.0.
    pub vignette: f32,
    /// Optional lateral chromatic-aberration correction, 0.0..=0.05.
    pub chromatic_aberration: f32,
    pub debug_overlay: bool,
    pub prefer_wired: bool,
    /// Opt-in only. No endpoint is hardcoded.
    pub crash_reporting: bool,
    pub crash_reporting_endpoint: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server: None,
            viewer_profile: None,
            refresh_rate_hz: None,
            resolution_cap: None,
            brightness: 1.0,
            vignette: 0.0,
            chromatic_aberration: 0.0,
            debug_overlay: false,
            prefer_wired: false,
            crash_reporting: false,
            crash_reporting_endpoint: None,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SettingsError {
    #[error("settings JSON is malformed: {0}")]
    Malformed(String),
}

impl Settings {
    pub fn from_json(json: &str) -> Result<Self, SettingsError> {
        let mut settings: Self = serde_json::from_str(json)
            .map_err(|error| SettingsError::Malformed(error.to_string()))?;
        settings.sanitize();
        Ok(settings)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Settings is always serialisable")
    }

    /// Clamp or drop values that would break the renderer or the session.
    pub fn sanitize(&mut self) {
        self.brightness = if self.brightness.is_finite() {
            self.brightness.clamp(0.05, 1.0)
        } else {
            1.0
        };
        self.vignette = if self.vignette.is_finite() {
            self.vignette.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.chromatic_aberration = if self.chromatic_aberration.is_finite() {
            self.chromatic_aberration.clamp(0.0, 0.05)
        } else {
            0.0
        };

        if let Some(rate) = self.refresh_rate_hz {
            if !rate.is_finite() || !(30.0..=240.0).contains(&rate) {
                self.refresh_rate_hz = None;
            }
        }

        self.resolution_cap = self.resolution_cap.and_then(|cap| {
            if cap.is_zero() {
                None
            } else {
                let aligned = cap.aligned_down();
                if aligned.is_zero() {
                    None
                } else {
                    Some(aligned)
                }
            }
        });

        if let Some(server) = &mut self.server {
            *server = server.trim().chars().take(255).collect();
            if server.is_empty() {
                self.server = None;
            }
        }

        if let Some(profile) = &self.viewer_profile {
            if profile.len() > MAX_VIEWER_PROFILE_BYTES {
                self.viewer_profile = None;
            }
        }

        if let Some(endpoint) = &mut self.crash_reporting_endpoint {
            *endpoint = endpoint.trim().chars().take(512).collect();
            let looks_like_http =
                endpoint.starts_with("https://") || endpoint.starts_with("http://");
            if endpoint.is_empty() || !looks_like_http {
                self.crash_reporting_endpoint = None;
            }
        }
    }

    /// Per-eye resolution to advertise, never larger than the panel.
    pub fn effective_view_resolution(&self, native: Resolution) -> Resolution {
        match self.resolution_cap {
            Some(cap) => cap.min(native).aligned_down(),
            None => native.aligned_down(),
        }
    }

    /// Diagnostics must never leak tokens. PhoneALVR has no secrets today, but
    /// the endpoint is user-controlled; keep it out.
    pub fn diagnostics_json(&self) -> String {
        let mut redacted = self.clone();
        redacted.crash_reporting_endpoint = redacted
            .crash_reporting_endpoint
            .as_ref()
            .map(|_| "<configured>".to_owned());
        redacted.to_json()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_sane() {
        let settings = Settings::default();
        assert_eq!(settings.brightness, 1.0);
        assert!(!settings.debug_overlay);
        assert!(!settings.crash_reporting);
    }

    #[test]
    fn json_round_trip() {
        let settings = Settings {
            server: Some("192.0.2.2".into()),
            viewer_profile: Some("{\"vendor\":\"Google\"}".into()),
            refresh_rate_hz: Some(90.0),
            resolution_cap: Some(Resolution::new(1000, 1000)),
            brightness: 0.5,
            vignette: 0.25,
            chromatic_aberration: 0.01,
            debug_overlay: true,
            prefer_wired: true,
            crash_reporting: false,
            crash_reporting_endpoint: None,
        };
        let decoded = Settings::from_json(&settings.to_json()).unwrap();
        // The cap is aligned down on sanitize.
        assert_eq!(decoded.resolution_cap, Some(Resolution::new(992, 992)));
        assert_eq!(decoded.server.as_deref(), Some("192.0.2.2"));
        assert!(decoded.debug_overlay);
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert!(matches!(
            Settings::from_json("{"),
            Err(SettingsError::Malformed(_))
        ));
    }

    #[test]
    fn invalid_values_are_clamped_or_dropped() {
        let json = r#"{
            "brightness": 99.0,
            "refresh_rate_hz": 500.0,
            "resolution_cap": {"width": 16, "height": 16},
            "server": "   ",
            "crash_reporting_endpoint": "javascript:alert(1)"
        }"#;
        let settings = Settings::from_json(json).unwrap();
        assert_eq!(settings.brightness, 1.0);
        assert_eq!(settings.refresh_rate_hz, None);
        assert_eq!(settings.resolution_cap, None);
        assert_eq!(settings.server, None);
        assert_eq!(settings.crash_reporting_endpoint, None);
    }

    #[test]
    fn nan_brightness_falls_back() {
        let mut settings = Settings {
            brightness: f32::NAN,
            ..Settings::default()
        };
        settings.sanitize();
        assert_eq!(settings.brightness, 1.0);
    }

    #[test]
    fn effective_resolution_is_capped_and_aligned() {
        let native = Resolution::new(1200, 1080);
        let settings = Settings::default();
        assert_eq!(
            settings.effective_view_resolution(native),
            Resolution::new(1184, 1056)
        );

        let mut settings = settings;
        settings.resolution_cap = Some(Resolution::new(800, 600));
        assert_eq!(
            settings.effective_view_resolution(native),
            Resolution::new(800, 576)
        );
    }

    #[test]
    fn unknown_fields_are_ignored_for_forward_compatibility() {
        let settings = Settings::from_json(r#"{"unknown_future_flag": true}"#).unwrap();
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn oversized_viewer_profile_is_dropped() {
        let mut settings = Settings {
            viewer_profile: Some("x".repeat(MAX_VIEWER_PROFILE_BYTES + 1)),
            ..Settings::default()
        };
        settings.sanitize();
        assert!(settings.viewer_profile.is_none());
    }
}
