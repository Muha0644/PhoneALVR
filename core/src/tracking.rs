//! 3DoF orientation tracking, angular-velocity estimation and short-horizon
//! prediction for late latching/timewarp.
//!
//! Android's rotation-vector sensor already fuses gyro/accelerometer/
//! magnetometer. The platform layer feeds its quaternion here at sensor rate;
//! this module keeps the math testable and independent of the sensor stack.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn scaled(self, factor: f32) -> Self {
        Self {
            x: self.x * factor,
            y: self.y * factor,
            z: self.z * factor,
        }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
            z: self.z + (other.z - self.z) * t,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const IDENTITY: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.x, self.y, self.z, self.w]
    }

    pub fn normalize(self) -> Self {
        let length = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        if length <= f32::EPSILON || !length.is_finite() {
            Self::IDENTITY
        } else {
            Self {
                x: self.x / length,
                y: self.y / length,
                z: self.z / length,
                w: self.w / length,
            }
        }
    }

    pub fn conjugate(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
            w: self.w,
        }
    }

    /// Hamilton product. `self * other` applies `other` first.
    #[allow(clippy::should_implement_trait)]
    pub fn mul(self, other: Self) -> Self {
        Self {
            x: self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            y: self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            z: self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
            w: self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
        }
    }

    pub fn from_axis_angle(axis: Vec3, angle_rad: f32) -> Self {
        let length = axis.length();
        if length <= f32::EPSILON || !angle_rad.is_finite() {
            return Self::IDENTITY;
        }
        let half = angle_rad * 0.5;
        let sin_half = half.sin();
        Self {
            x: axis.x / length * sin_half,
            y: axis.y / length * sin_half,
            z: axis.z / length * sin_half,
            w: half.cos(),
        }
        .normalize()
    }

    /// Quaternion for a rotation vector in radians (axis * angle).
    pub fn from_scaled_axis(rotation_vector: Vec3) -> Self {
        let angle = rotation_vector.length();
        if angle <= f32::EPSILON {
            Self::IDENTITY
        } else {
            Self::from_axis_angle(rotation_vector, angle)
        }
    }

    /// Rotates a vector by this unit quaternion.
    pub fn rotate_vec3(self, vector: Vec3) -> Vec3 {
        let qv = Self::new(vector.x, vector.y, vector.z, 0.0);
        let result = self.mul(qv).mul(self.conjugate());
        Vec3::new(result.x, result.y, result.z)
    }

    pub fn angle_to(self, other: Self) -> f32 {
        let dot = (self.x * other.x + self.y * other.y + self.z * other.z + self.w * other.w)
            .abs()
            .min(1.0);
        2.0 * dot.acos()
    }
}

/// Convert an Android `TYPE_ROTATION_VECTOR` quaternion into an OpenVR head
/// pose.
///
/// Android reports the rotation from the device's natural (portrait) frame to
/// an East-North-Up world frame. OpenVR expects a Y-up frame with forward along
/// `-Z`. When the phone is mounted in landscape, the sensor frame is still the
/// natural portrait frame, so the display rotation must be applied as well.
/// `display_rotation` is `android.view.Surface.ROTATION_*` (0, 90, 180, 270)
/// divided by 90.
///
/// With the screen facing the user's eyes, the displayed frame *is* the
/// headset frame, so this maps the sensor into that frame.
pub fn android_orientation_to_openvr(orientation: Quat, display_rotation: u8) -> Quat {
    // `Surface.ROTATION_*` is the rotation of the drawn UI relative to the
    // device, so the device-to-display rotation is a positive Z rotation.
    let display = Quat::from_axis_angle(
        Vec3::new(0.0, 0.0, 1.0),
        (display_rotation.min(3) as f32) * std::f32::consts::FRAC_PI_2,
    );
    let enu_to_openvr =
        Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), -std::f32::consts::FRAC_PI_2);
    enu_to_openvr
        .mul(orientation.normalize())
        .mul(display.conjugate())
        .normalize()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImuSample {
    pub orientation: Quat,
    pub timestamp_ns: u64,
}

#[derive(Debug, Clone)]
pub struct HeadTracker {
    orientation: Quat,
    angular_velocity: Vec3,
    recenter_offset: Quat,
    last_timestamp_ns: Option<u64>,
    max_prediction_s: f32,
    velocity_smoothing: f32,
}

impl Default for HeadTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl HeadTracker {
    pub fn new() -> Self {
        Self {
            orientation: Quat::IDENTITY,
            angular_velocity: Vec3::ZERO,
            recenter_offset: Quat::IDENTITY,
            last_timestamp_ns: None,
            max_prediction_s: 0.100,
            velocity_smoothing: 0.35,
        }
    }

    /// Feed a rotation-vector sample. Timestamps must be monotonic nanoseconds.
    pub fn ingest(&mut self, sample: ImuSample) {
        let orientation = sample.orientation.normalize();

        if let Some(previous_ns) = self.last_timestamp_ns {
            let dt = sample.timestamp_ns.saturating_sub(previous_ns) as f32 / 1_000_000_000.0;
            if dt > 1e-5 && dt < 2.0 {
                let mut delta = orientation.mul(self.orientation.conjugate()).normalize();
                // q and -q describe the same rotation; choose the short arc.
                if delta.w < 0.0 {
                    delta = Quat::new(-delta.x, -delta.y, -delta.z, -delta.w);
                }
                let sin_half = (delta.x * delta.x + delta.y * delta.y + delta.z * delta.z).sqrt();
                let angular_velocity = if sin_half > 1e-6 {
                    let angle = 2.0 * sin_half.atan2(delta.w);
                    let scale = angle / sin_half / dt;
                    Vec3::new(delta.x * scale, delta.y * scale, delta.z * scale)
                } else {
                    Vec3::ZERO
                };
                self.angular_velocity = self
                    .angular_velocity
                    .lerp(angular_velocity, self.velocity_smoothing);
            }
        }

        self.orientation = orientation;
        self.last_timestamp_ns = Some(sample.timestamp_ns);
    }

    /// Pose for a presentation timestamp. The prediction horizon is clamped so
    /// a stale sensor or a bogus timestamp cannot produce wild motion.
    pub fn pose_at(&self, target_timestamp_ns: u64) -> Quat {
        let Some(last_ns) = self.last_timestamp_ns else {
            return self.recenter_offset;
        };

        let dt = if target_timestamp_ns >= last_ns {
            ((target_timestamp_ns - last_ns) as f32 / 1_000_000_000.0).min(self.max_prediction_s)
        } else {
            // Slightly in the past is allowed for late-latched buffers; never
            // extrapolate backwards.
            -(((last_ns - target_timestamp_ns) as f32 / 1_000_000_000.0).min(0.05))
        };

        let predicted = if dt.abs() <= 1e-6 {
            self.orientation
        } else {
            Quat::from_scaled_axis(self.angular_velocity.scaled(dt)).mul(self.orientation)
        };

        (self.recenter_offset.mul(predicted)).normalize()
    }

    /// Make the current orientation forward. Safe to call at any time.
    pub fn recenter(&mut self) {
        self.recenter_offset = self.orientation.conjugate().normalize();
        self.angular_velocity = Vec3::ZERO;
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn orientation(&self) -> Quat {
        self.orientation
    }

    pub fn angular_velocity(&self) -> Vec3 {
        self.angular_velocity
    }

    pub fn last_timestamp_ns(&self) -> Option<u64> {
        self.last_timestamp_ns
    }

    pub fn set_max_prediction(&mut self, seconds: f32) {
        if seconds.is_finite() {
            self.max_prediction_s = seconds.clamp(0.0, 0.250);
        }
    }

    pub fn set_velocity_smoothing(&mut self, alpha: f32) {
        if alpha.is_finite() {
            self.velocity_smoothing = alpha.clamp(0.01, 1.0);
        }
    }
}

/// Approximate 2D late-latching offset for a decoded frame.
///
/// `current` is the newest predicted head orientation at presentation time and
/// `frame` is the head orientation ALVR used for the decoded frame. The result
/// is an offset in normalized eye-viewport UV, clamped so a bad timestamp
/// cannot tear the image across the screen. This is a translation-only
/// reprojection; a full depth-aware timewarp is future work.
pub fn reprojection_offset(
    current: Quat,
    frame: Quat,
    tan_half_horizontal: f32,
    tan_half_vertical: f32,
) -> [f32; 2] {
    if !tan_half_horizontal.is_finite()
        || !tan_half_vertical.is_finite()
        || tan_half_horizontal <= 1e-4
        || tan_half_vertical <= 1e-4
    {
        return [0.0, 0.0];
    }
    let delta = current
        .normalize()
        .mul(frame.normalize().conjugate())
        .normalize();
    // Yaw around Y and pitch around X from the delta quaternion.
    let yaw = (2.0 * (delta.w * delta.y + delta.z * delta.x))
        .atan2(1.0 - 2.0 * (delta.y * delta.y + delta.z * delta.z));
    let pitch = (2.0 * (delta.w * delta.x - delta.y * delta.z))
        .clamp(-1.0, 1.0)
        .asin();
    let offset = [
        -(yaw.tan()) / (2.0 * tan_half_horizontal),
        (pitch.tan()) / (2.0 * tan_half_vertical),
    ];
    [offset[0].clamp(-0.25, 0.25), offset[1].clamp(-0.25, 0.25)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn yaw(angle: f32) -> Quat {
        Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), angle)
    }

    #[test]
    fn android_portrait_upright_maps_to_openvr_identity() {
        // Phone held upright in front of the user, screen facing them.
        let device = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), FRAC_PI_2);
        let head = android_orientation_to_openvr(device, 0);
        assert!(head.angle_to(Quat::IDENTITY) < 1e-4, "{head:?}");
    }

    #[test]
    fn android_landscape_rotations_map_to_openvr_identity() {
        let base = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), FRAC_PI_2);
        for (rotation, roll) in [(1u8, FRAC_PI_2), (3u8, -FRAC_PI_2)] {
            let device = base.mul(Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), roll));
            let head = android_orientation_to_openvr(device, rotation);
            assert!(head.angle_to(Quat::IDENTITY) < 1e-4, "{rotation}: {head:?}");
        }
    }

    #[test]
    fn android_world_yaw_maps_to_openvr_yaw() {
        let base = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), FRAC_PI_2);
        let device = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), FRAC_PI_2).mul(base);
        let head = android_orientation_to_openvr(device, 0);
        let expected = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), FRAC_PI_2);
        assert!(head.angle_to(expected) < 1e-4, "{head:?}");
    }

    #[test]
    fn display_rotation_is_clamped() {
        let device = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), FRAC_PI_2);
        assert_eq!(
            android_orientation_to_openvr(device, 7),
            android_orientation_to_openvr(device, 3)
        );
    }

    #[test]
    fn identity_pose_without_samples() {
        let tracker = HeadTracker::new();
        assert_eq!(tracker.pose_at(123), Quat::IDENTITY);
    }

    #[test]
    fn ninety_degrees_per_second_is_estimated() {
        let mut tracker = HeadTracker::new();
        tracker.set_velocity_smoothing(1.0);
        tracker.set_max_prediction(1.0);
        tracker.ingest(ImuSample {
            orientation: Quat::IDENTITY,
            timestamp_ns: 0,
        });
        tracker.ingest(ImuSample {
            orientation: yaw(FRAC_PI_2),
            timestamp_ns: 1_000_000_000,
        });
        let omega = tracker.angular_velocity();
        assert!((omega.y - FRAC_PI_2).abs() < 0.01, "omega={omega:?}");
        assert!(omega.x.abs() < 1e-4);
        assert!(omega.z.abs() < 1e-4);
    }

    #[test]
    fn prediction_advances_orientation_by_velocity() {
        let mut tracker = HeadTracker::new();
        tracker.set_velocity_smoothing(1.0);
        tracker.set_max_prediction(1.0);
        tracker.ingest(ImuSample {
            orientation: Quat::IDENTITY,
            timestamp_ns: 0,
        });
        tracker.ingest(ImuSample {
            orientation: yaw(FRAC_PI_2),
            timestamp_ns: 1_000_000_000,
        });

        let predicted = tracker.pose_at(1_010_000_000);
        let expected = yaw(FRAC_PI_2 * 1.01);
        assert!(
            predicted.angle_to(expected) < 0.01,
            "predicted={predicted:?} expected={expected:?}"
        );
    }

    #[test]
    fn prediction_is_clamped() {
        let mut tracker = HeadTracker::new();
        tracker.set_velocity_smoothing(1.0);
        tracker.set_max_prediction(0.020);
        tracker.ingest(ImuSample {
            orientation: Quat::IDENTITY,
            timestamp_ns: 0,
        });
        tracker.ingest(ImuSample {
            orientation: yaw(FRAC_PI_2),
            timestamp_ns: 1_000_000_000,
        });
        let clamped = tracker.pose_at(10_000_000_000);
        let expected = yaw(FRAC_PI_2 * 1.020);
        assert!(clamped.angle_to(expected) < 0.01);
    }

    #[test]
    fn recenter_makes_current_orientation_forward() {
        let mut tracker = HeadTracker::new();
        tracker.ingest(ImuSample {
            orientation: yaw(1.0),
            timestamp_ns: 0,
        });
        tracker.recenter();
        assert!(tracker.pose_at(0).angle_to(Quat::IDENTITY) < 1e-5);
    }

    #[test]
    fn shortest_arc_handles_negated_quaternions() {
        let mut tracker = HeadTracker::new();
        tracker.set_velocity_smoothing(1.0);
        tracker.ingest(ImuSample {
            orientation: Quat::IDENTITY,
            timestamp_ns: 0,
        });
        let negated = Quat::new(0.0, 0.0, 0.0, -1.0);
        tracker.ingest(ImuSample {
            orientation: negated,
            timestamp_ns: 1_000_000_000,
        });
        assert!(tracker.angular_velocity().length() < 1e-4);
    }

    #[test]
    fn non_finite_quaternion_falls_back_to_identity() {
        let mut tracker = HeadTracker::new();
        tracker.ingest(ImuSample {
            orientation: Quat::new(f32::NAN, 0.0, 0.0, 0.0),
            timestamp_ns: 0,
        });
        assert_eq!(tracker.orientation(), Quat::IDENTITY);
    }

    #[test]
    fn reprojection_is_zero_for_same_pose() {
        let offset = reprojection_offset(Quat::IDENTITY, Quat::IDENTITY, 1.0, 1.0);
        assert!(offset[0].abs() < 1e-6);
        assert!(offset[1].abs() < 1e-6);
    }

    #[test]
    fn reprojection_moves_opposite_a_right_yaw() {
        let current = yaw(0.1);
        let offset = reprojection_offset(current, Quat::IDENTITY, 1.0, 1.0);
        assert!(offset[0] < 0.0, "offset={offset:?}");
    }

    #[test]
    fn reprojection_is_clamped() {
        let current = yaw(1.5);
        let offset = reprojection_offset(current, Quat::IDENTITY, 0.2, 0.2);
        assert!(offset[0] >= -0.25 && offset[0] <= 0.25);
    }

    #[test]
    fn quaternion_rotation_round_trip() {
        let q = yaw(PI / 3.0);
        let rotated = q.rotate_vec3(Vec3::new(1.0, 0.0, 0.0));
        let back = q.conjugate().rotate_vec3(rotated);
        assert!((back.x - 1.0).abs() < 1e-5);
        assert!(back.z.abs() < 1e-5);
    }
}
