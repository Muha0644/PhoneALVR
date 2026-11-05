//! View-parameter deduplication.
//!
//! ALVR 20.12+ sends a `ViewsConfig` packet on *every* `send_view_params` call.
//! The streamer turns each packet into SteamVR display-property updates and a
//! `VREvent_LensDistortionChanged`, which can trigger a warp-mesh rebuild
//! storm. View parameters only change when the viewer profile, IPD or display
//! size changes, so the platform layer must send them when they change — never
//! at the tracking rate.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fov {
    pub left: f32,
    pub right: f32,
    pub up: f32,
    pub down: f32,
}

impl Fov {
    pub const fn new(left: f32, right: f32, up: f32, down: f32) -> Self {
        Self {
            left,
            right,
            up,
            down,
        }
    }

    pub fn is_finite(self) -> bool {
        self.left.is_finite()
            && self.right.is_finite()
            && self.up.is_finite()
            && self.down.is_finite()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EyeView {
    /// Eye offset relative to the head, in metres.
    pub position: [f32; 3],
    pub fov: Fov,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewParams(pub [EyeView; 2]);

impl ViewParams {
    pub fn is_finite(&self) -> bool {
        self.0
            .iter()
            .all(|eye| eye.position.iter().all(|value| value.is_finite()) && eye.fov.is_finite())
    }

    fn approximately_equals(&self, other: &Self, epsilon: f32) -> bool {
        self.0.iter().zip(other.0.iter()).all(|(a, b)| {
            a.position
                .iter()
                .zip(b.position.iter())
                .all(|(x, y)| (x - y).abs() <= epsilon)
                && (a.fov.left - b.fov.left).abs() <= epsilon
                && (a.fov.right - b.fov.right).abs() <= epsilon
                && (a.fov.up - b.fov.up).abs() <= epsilon
                && (a.fov.down - b.fov.down).abs() <= epsilon
        })
    }
}

/// Tracks the last sent view parameters. Initial state always sends once.
#[derive(Debug, Clone)]
pub struct ViewParamDedup {
    last_sent: Option<ViewParams>,
    epsilon: f32,
}

impl Default for ViewParamDedup {
    fn default() -> Self {
        Self::new(1e-4)
    }
}

impl ViewParamDedup {
    pub fn new(epsilon: f32) -> Self {
        Self {
            last_sent: None,
            epsilon: if epsilon.is_finite() && epsilon >= 0.0 {
                epsilon
            } else {
                1e-4
            },
        }
    }

    /// Returns true when the caller should call `send_view_params`, and records
    /// the value as sent. Non-finite values are rejected (returns false).
    pub fn should_send(&mut self, current: ViewParams) -> bool {
        if !current.is_finite() {
            return false;
        }
        if let Some(last) = self.last_sent {
            if current.approximately_equals(&last, self.epsilon) {
                return false;
            }
        }
        self.last_sent = Some(current);
        true
    }

    /// Force the next call to send, for example after a reconnect.
    pub fn invalidate(&mut self) {
        self.last_sent = None;
    }

    pub fn last_sent(&self) -> Option<ViewParams> {
        self.last_sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(ipd: f32, fov_scale: f32) -> ViewParams {
        ViewParams([
            EyeView {
                position: [-ipd / 2.0, 0.0, 0.0],
                fov: Fov::new(-fov_scale, fov_scale, 0.9 * fov_scale, -0.9 * fov_scale),
            },
            EyeView {
                position: [ipd / 2.0, 0.0, 0.0],
                fov: Fov::new(-fov_scale, fov_scale, 0.9 * fov_scale, -0.9 * fov_scale),
            },
        ])
    }

    #[test]
    fn first_value_is_sent() {
        let mut dedup = ViewParamDedup::default();
        assert!(dedup.should_send(params(0.064, 1.0)));
    }

    #[test]
    fn identical_values_are_not_spammed() {
        let mut dedup = ViewParamDedup::default();
        assert!(dedup.should_send(params(0.064, 1.0)));
        for _ in 0..1000 {
            assert!(!dedup.should_send(params(0.064, 1.0)));
        }
    }

    #[test]
    fn sub_epsilon_noise_is_ignored() {
        let mut dedup = ViewParamDedup::default();
        assert!(dedup.should_send(params(0.064, 1.0)));
        assert!(!dedup.should_send(params(0.064 + 5e-5, 1.0)));
    }

    #[test]
    fn real_changes_are_sent() {
        let mut dedup = ViewParamDedup::default();
        assert!(dedup.should_send(params(0.064, 1.0)));
        assert!(dedup.should_send(params(0.070, 1.0)));
        assert!(dedup.should_send(params(0.070, 1.05)));
    }

    #[test]
    fn invalidate_forces_the_next_send() {
        let mut dedup = ViewParamDedup::default();
        assert!(dedup.should_send(params(0.064, 1.0)));
        dedup.invalidate();
        assert!(dedup.should_send(params(0.064, 1.0)));
    }

    #[test]
    fn non_finite_values_are_never_sent() {
        let mut dedup = ViewParamDedup::default();
        let mut bad = params(0.064, 1.0);
        bad.0[0].fov.left = f32::NAN;
        assert!(!dedup.should_send(bad));
        assert!(dedup.last_sent().is_none());
    }
}
