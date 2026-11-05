//! Lens distortion and per-eye sampling math.
//!
//! The Cardboard profile defines the radial mapping
//!
//! ```text
//! p' = p * (1 + K1 r^2 + K2 r^4 + ... + Kn r^(2n))
//! ```
//!
//! where `p` is a point on the real display and `p'` is the corresponding
//! point in the rendered texture, both in tangent-angle units relative to the
//! lens centre. The renderer builds a distortion mesh by evaluating this map
//! for each output vertex; the fragment shader only has to sample the texture.

use crate::profile::{EyeFov, TanFov, ViewerProfile};

const MAX_COEFFICIENTS: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct LensDistortion {
    coefficients: Vec<f32>,
}

impl Default for LensDistortion {
    fn default() -> Self {
        Self::new(&[0.441, 0.156, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])
    }
}

impl LensDistortion {
    pub fn new(coefficients: &[f32]) -> Self {
        let mut values: Vec<f32> = coefficients
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .take(MAX_COEFFICIENTS)
            .collect();
        if values.is_empty() {
            values.push(0.0);
        }
        Self {
            coefficients: values,
        }
    }

    pub fn coefficients(&self) -> &[f32] {
        &self.coefficients
    }

    /// Radial distortion factor for a tangent-space radius.
    pub fn factor_for_tan_radius(&self, radius: f32) -> f32 {
        let r2 = radius * radius;
        let mut factor = 1.0;
        let mut power = r2;
        for coefficient in &self.coefficients {
            factor += coefficient * power;
            power *= r2;
        }
        factor
    }

    /// `p' = distort(p)` in tangent-angle units.
    pub fn distort_tan(&self, x: f32, y: f32) -> [f32; 2] {
        let radius = (x * x + y * y).sqrt();
        if radius <= f32::EPSILON {
            return [x, y];
        }
        let factor = self.factor_for_tan_radius(radius);
        [x * factor, y * factor]
    }

    /// Numeric inverse. Used for tests, calibration scenes and optional mesh
    /// precomputation. The production path only needs the forward map.
    pub fn undistort_tan(&self, x: f32, y: f32) -> [f32; 2] {
        let target_radius = (x * x + y * y).sqrt();
        if target_radius <= f32::EPSILON {
            return [x, y];
        }

        // Find r such that r * factor(r^2) = target_radius.
        let mut low = 0.0f32;
        let mut high = target_radius.max(0.01);
        for _ in 0..32 {
            if high * self.factor_for_tan_radius(high) >= target_radius {
                break;
            }
            high *= 1.5;
            if high > 100.0 {
                break;
            }
        }
        for _ in 0..48 {
            let mid = (low + high) * 0.5;
            let value = mid * self.factor_for_tan_radius(mid);
            if value < target_radius {
                low = mid;
            } else {
                high = mid;
            }
        }
        let radius = (low + high) * 0.5;
        let scale = radius / target_radius;
        [x * scale, y * scale]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Red,
    Green,
    Blue,
}

impl Channel {
    /// Chromatic-aberration sign. Red is magnified slightly more than green,
    /// blue slightly less; this matches the usual lateral-CA direction.
    fn offset(self) -> f32 {
        match self {
            Self::Red => 1.0,
            Self::Green => 0.0,
            Self::Blue => -1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EyeMapper {
    pub fov: EyeFov,
    /// Lens centre in normalised eye-viewport coordinates. [0.5, 0.5] is the
    /// geometric centre of this eye's half-screen.
    pub lens_center_uv: [f32; 2],
    pub distortion: LensDistortion,
    /// 0.0 disables chromatic-aberration correction.
    pub chromatic_aberration: f32,
}

impl EyeMapper {
    pub fn from_profile(profile: &ViewerProfile) -> Self {
        Self {
            fov: profile.fov,
            lens_center_uv: [0.5, 0.5],
            distortion: LensDistortion::new(&profile.distortion_coefficients),
            chromatic_aberration: 0.0,
        }
    }

    pub fn with_chromatic_aberration(mut self, amount: f32) -> Self {
        self.chromatic_aberration = if amount.is_finite() {
            amount.clamp(0.0, 0.05)
        } else {
            0.0
        };
        self
    }

    /// Map an output pixel in [0,1] eye-viewport coordinates to the source
    /// texture coordinate. Values are clamped to the texture bounds.
    pub fn source_uv(&self, out_uv: [f32; 2]) -> [f32; 2] {
        self.source_uv_for_channel(out_uv, Channel::Green)
    }

    pub fn source_uv_for_channel(&self, out_uv: [f32; 2], channel: Channel) -> [f32; 2] {
        let tan_fov = self.fov.tan_extents();
        let tan = tan_fov.uv_to_tan(out_uv);
        let center_tan = tan_fov.uv_to_tan(self.lens_center_uv);
        let relative = [tan[0] - center_tan[0], tan[1] - center_tan[1]];

        let relative = if self.chromatic_aberration > 0.0 && channel != Channel::Green {
            let scale = 1.0 + self.chromatic_aberration * channel.offset();
            [relative[0] * scale, relative[1] * scale]
        } else {
            relative
        };

        let distorted = self.distortion.distort_tan(relative[0], relative[1]);
        let source_tan = [distorted[0] + center_tan[0], distorted[1] + center_tan[1]];
        let uv = tan_fov.tan_to_uv(source_tan);
        [uv[0].clamp(0.0, 1.0), uv[1].clamp(0.0, 1.0)]
    }

    pub fn tan_fov(&self) -> TanFov {
        self.fov.tan_extents()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(left: f32, right: f32, epsilon: f32) -> bool {
        (left - right).abs() <= epsilon
    }

    #[test]
    fn zero_coefficients_are_identity() {
        let distortion = LensDistortion::new(&[0.0; 8]);
        let point = [0.3, -0.4];
        let result = distortion.distort_tan(point[0], point[1]);
        assert!(approx(result[0], point[0], 1e-6));
        assert!(approx(result[1], point[1], 1e-6));
    }

    #[test]
    fn known_cardboard_v1_factor() {
        let distortion = LensDistortion::new(&[0.441, 0.156, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        // r = 1: 1 + 0.441 + 0.156 = 1.597
        assert!(approx(distortion.factor_for_tan_radius(1.0), 1.597, 1e-6));
        // r = 0.5: r2 = 0.25, r4 = 0.0625 => 1 + 0.11025 + 0.00975
        assert!(approx(distortion.factor_for_tan_radius(0.5), 1.12, 1e-6));
    }

    #[test]
    fn distort_scales_the_radius() {
        let distortion = LensDistortion::new(&[0.5, 0.0]);
        let result = distortion.distort_tan(0.8, 0.0);
        // 1 + 0.5 * 0.64 = 1.32
        assert!(approx(result[0], 0.8 * 1.32, 1e-6));
        assert!(approx(result[1], 0.0, 1e-6));
    }

    #[test]
    fn inverse_round_trip() {
        let distortion = LensDistortion::new(&[0.441, 0.156, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        for point in [[0.1, 0.2], [-0.4, 0.3], [0.7, -0.2]] {
            let distorted = distortion.distort_tan(point[0], point[1]);
            let restored = distortion.undistort_tan(distorted[0], distorted[1]);
            assert!(approx(restored[0], point[0], 1e-4), "{restored:?}");
            assert!(approx(restored[1], point[1], 1e-4), "{restored:?}");
        }
    }

    #[test]
    fn centre_maps_to_centre() {
        let profile = ViewerProfile::cardboard_v1();
        let mapper = EyeMapper::from_profile(&profile);
        let uv = mapper.source_uv([0.5, 0.5]);
        assert!(approx(uv[0], 0.5, 1e-5));
        assert!(approx(uv[1], 0.5, 1e-5));
    }

    #[test]
    fn distortion_pulls_peripheral_samples_inward_for_positive_coefficients() {
        let profile = ViewerProfile::cardboard_v1();
        let mapper = EyeMapper::from_profile(&profile);
        // Positive coefficients magnify the radius: a peripheral output
        // sample reads further out in the source texture.
        let edge = mapper.source_uv([0.8, 0.5]);
        assert!(edge[0] > 0.8);
        assert!(edge[0] <= 1.0);
        let opposite = mapper.source_uv([0.2, 0.5]);
        assert!(opposite[0] < 0.2);
        assert!(opposite[0] >= 0.0);
    }

    #[test]
    fn output_is_always_clamped_to_texture() {
        let profile = ViewerProfile::cardboard_v1();
        let mapper = EyeMapper::from_profile(&profile);
        for uv in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0], [0.5, 0.5]] {
            let source = mapper.source_uv(uv);
            assert!((0.0..=1.0).contains(&source[0]));
            assert!((0.0..=1.0).contains(&source[1]));
        }
    }

    #[test]
    fn chromatic_aberration_offsets_channels() {
        let profile = ViewerProfile::cardboard_v1();
        let mapper = EyeMapper::from_profile(&profile).with_chromatic_aberration(0.01);
        let uv = [0.7, 0.5];
        let red = mapper.source_uv_for_channel(uv, Channel::Red);
        let green = mapper.source_uv_for_channel(uv, Channel::Green);
        let blue = mapper.source_uv_for_channel(uv, Channel::Blue);
        assert!(red[0] > green[0]);
        assert!(green[0] > blue[0]);
    }

    #[test]
    fn tan_uv_round_trip() {
        let fov = EyeFov::new(50.0, 50.0, 40.0, 40.0);
        let tan = fov.tan_extents();
        let original = [0.25, 0.75];
        let round_trip = tan.tan_to_uv(tan.uv_to_tan(original));
        assert!(approx(round_trip[0], original[0], 1e-5));
        assert!(approx(round_trip[1], original[1], 1e-5));
    }
}
