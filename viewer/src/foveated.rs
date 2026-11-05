//! Port of ALVR v20.14.1's foveated-encoding (FFE) shader mapping.
//!
//! ALVR encodes a lower-resolution image with a high-resolution centre region
//! and horizontally/vertically compressed edges. The client must map its
//! eye-image UV back through the FFE layout before sampling the decoded
//! texture. This is the exact math from
//! `alvr/graphics/src/stream.rs::foveated_encoding_shader_constants` and
//! `alvr/graphics/resources/stream.wgsl`, kept in Rust so it can be tested
//! without a device or a GL context.
//!
//! Integration status: the math and configuration are implemented here; the
//! renderer shader hookup is the next vertical slice (see
//! `docs/dev/FFE_HANDOFF.md`).

use serde::{Deserialize, Serialize};

/// Viewer/server foveation parameters. Defaults match ALVR's default
/// `FoveatedEncodingConfig`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FoveatedConfig {
    pub center_size_x: f32,
    pub center_size_y: f32,
    pub center_shift_x: f32,
    pub center_shift_y: f32,
    pub edge_ratio_x: f32,
    pub edge_ratio_y: f32,
}

impl Default for FoveatedConfig {
    fn default() -> Self {
        Self {
            center_size_x: 0.45,
            center_size_y: 0.4,
            center_shift_x: 0.4,
            center_shift_y: 0.1,
            edge_ratio_x: 4.0,
            edge_ratio_y: 5.0,
        }
    }
}

impl FoveatedConfig {
    /// Clamp to the server's schema ranges so malformed settings cannot
    /// produce NaN/Inf shader constants.
    pub fn sanitized(self) -> Self {
        let finite = |value: f32, fallback: f32| {
            if value.is_finite() {
                value
            } else {
                fallback
            }
        };
        Self {
            center_size_x: finite(self.center_size_x, 0.45).clamp(0.0, 1.0),
            center_size_y: finite(self.center_size_y, 0.4).clamp(0.0, 1.0),
            center_shift_x: finite(self.center_shift_x, 0.0).clamp(-1.0, 1.0),
            center_shift_y: finite(self.center_shift_y, 0.0).clamp(-1.0, 1.0),
            edge_ratio_x: finite(self.edge_ratio_x, 1.0).clamp(1.0, 10.0),
            edge_ratio_y: finite(self.edge_ratio_y, 1.0).clamp(1.0, 10.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Axis {
    view: f32,
    aligned: f32,
    view_ratio: f32,
    edge_ratio: f32,
    c1: f32,
    c2: f32,
    lo_bound: f32,
    hi_bound: f32,
    a_left: f32,
    b_left: f32,
    a_right: f32,
    b_right: f32,
    c_right: f32,
}

impl Axis {
    fn new(view: f32, center_size: f32, center_shift: f32, edge_ratio: f32) -> Self {
        let view = view.max(1.0);
        let edge_ratio = edge_ratio.max(1.0);
        let edge_size = view - center_size * view;
        let center_size_aligned =
            1.0 - (edge_size / (edge_ratio * 2.0)).ceil() * (edge_ratio * 2.0) / view;
        let edge_size_aligned = view - center_size_aligned * view;
        let center_shift_aligned = if edge_size_aligned.abs() <= f32::EPSILON {
            0.0
        } else {
            (center_shift * edge_size_aligned / (edge_ratio * 2.0)).ceil() * (edge_ratio * 2.0)
                / edge_size_aligned
        };

        let foveation_scale = center_size_aligned + (1.0 - center_size_aligned) / edge_ratio;
        let optimized = foveation_scale * view;
        let aligned = ((optimized / 32.0).ceil() * 32.0).max(32.0);
        let view_ratio = optimized / aligned;

        let c0 = (1.0 - center_size_aligned) * 0.5;
        let c1 = (edge_ratio - 1.0) * c0 * (center_shift_aligned + 1.0) / edge_ratio;
        let c2 = (edge_ratio - 1.0) * center_size_aligned + 1.0;
        let lo_bound = c0 * (center_shift_aligned + 1.0);
        let hi_bound = c0 * (center_shift_aligned - 1.0) + 1.0;
        let lo_bound_c = c0 * (center_shift_aligned + 1.0) / c2;
        let hi_bound_c = c0 * (center_shift_aligned - 1.0) / c2 + 1.0;

        let a_left = c2 * (1.0 - edge_ratio) / (edge_ratio * lo_bound_c);
        let b_left = (c1 + c2 * lo_bound_c) / lo_bound_c;
        let a_right = c2 * (edge_ratio - 1.0) / (edge_ratio * (1.0 - hi_bound_c));
        let b_right = (c2 - edge_ratio * c1 - 2.0 * edge_ratio * c2
            + c2 * edge_ratio * (1.0 - hi_bound_c)
            + edge_ratio)
            / (edge_ratio * (1.0 - hi_bound_c));
        let c_right = (c2 * edge_ratio - c2) * (c1 - hi_bound_c + c2 * hi_bound_c)
            / (edge_ratio * (1.0 - hi_bound_c) * (1.0 - hi_bound_c));

        Self {
            view,
            aligned,
            view_ratio,
            edge_ratio,
            c1,
            c2,
            lo_bound,
            hi_bound,
            a_left,
            b_left,
            a_right,
            b_right,
            c_right,
        }
    }

    fn encoded_resolution(&self) -> u32 {
        self.aligned.max(32.0) as u32
    }

    fn map(&self, value: f32) -> f32 {
        let center = (value - self.c1) * self.edge_ratio / self.c2;
        let left_edge = if self.a_left.abs() <= f32::EPSILON {
            value
        } else {
            let discriminant = self.b_left * self.b_left + 4.0 * self.a_left * value;
            (-self.b_left + discriminant.max(0.0).sqrt()) / (2.0 * self.a_left)
        };
        let right_edge = if self.a_right.abs() <= f32::EPSILON {
            value
        } else {
            let discriminant =
                self.b_right * self.b_right - 4.0 * (self.c_right - self.a_right * value);
            (-self.b_right + discriminant.max(0.0).sqrt()) / (2.0 * self.a_right)
        };

        let mapped = if value < self.lo_bound {
            left_edge
        } else if value > self.hi_bound {
            right_edge
        } else {
            center
        };
        (mapped * self.view_ratio).clamp(0.0, 1.0)
    }
}

/// FFE layout for one negotiated stream. `base_resolution` is the per-eye
/// resolution requested by the streamer; `encoded_resolution` is the smaller
/// per-eye texture that the decoder should receive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoveatedEncoding {
    pub base_resolution: [u32; 2],
    pub encoded_resolution: [u32; 2],
    pub view_ratio: [f32; 2],
    pub edge_ratio: [f32; 2],
    pub c1: [f32; 2],
    pub c2: [f32; 2],
    pub lo_bound: [f32; 2],
    pub hi_bound: [f32; 2],
    pub a_left: [f32; 2],
    pub b_left: [f32; 2],
    pub a_right: [f32; 2],
    pub b_right: [f32; 2],
    pub c_right: [f32; 2],
}

impl FoveatedEncoding {
    pub fn new(base_resolution: [u32; 2], config: FoveatedConfig) -> Self {
        let config = config.sanitized();
        let x = Axis::new(
            base_resolution[0] as f32,
            config.center_size_x,
            config.center_shift_x,
            config.edge_ratio_x,
        );
        let y = Axis::new(
            base_resolution[1] as f32,
            config.center_size_y,
            config.center_shift_y,
            config.edge_ratio_y,
        );
        Self {
            base_resolution,
            encoded_resolution: [x.encoded_resolution(), y.encoded_resolution()],
            view_ratio: [x.view_ratio, y.view_ratio],
            edge_ratio: [config.edge_ratio_x, config.edge_ratio_y],
            c1: [x.c1, y.c1],
            c2: [x.c2, y.c2],
            lo_bound: [x.lo_bound, y.lo_bound],
            hi_bound: [x.hi_bound, y.hi_bound],
            a_left: [x.a_left, y.a_left],
            b_left: [x.b_left, y.b_left],
            a_right: [x.a_right, y.a_right],
            b_right: [x.b_right, y.b_right],
            c_right: [x.c_right, y.c_right],
        }
    }

    /// Map an eye-image UV in [0,1] to the encoded texture UV. `eye` is 0 for
    /// left and 1 for right; ALVR mirrors the right eye horizontally around
    /// the FFE transform.
    pub fn map_eye_uv(&self, eye: usize, uv: [f32; 2]) -> [f32; 2] {
        let mut input = uv;
        if eye == 1 {
            input[0] = 1.0 - input[0];
        }
        let x = Axis {
            view: self.base_resolution[0] as f32,
            aligned: self.encoded_resolution[0] as f32,
            view_ratio: self.view_ratio[0],
            edge_ratio: self.edge_ratio[0],
            c1: self.c1[0],
            c2: self.c2[0],
            lo_bound: self.lo_bound[0],
            hi_bound: self.hi_bound[0],
            a_left: self.a_left[0],
            b_left: self.b_left[0],
            a_right: self.a_right[0],
            b_right: self.b_right[0],
            c_right: self.c_right[0],
        };
        let y = Axis {
            view: self.base_resolution[1] as f32,
            aligned: self.encoded_resolution[1] as f32,
            view_ratio: self.view_ratio[1],
            edge_ratio: self.edge_ratio[1],
            c1: self.c1[1],
            c2: self.c2[1],
            lo_bound: self.lo_bound[1],
            hi_bound: self.hi_bound[1],
            a_left: self.a_left[1],
            b_left: self.b_left[1],
            a_right: self.a_right[1],
            b_right: self.b_right[1],
            c_right: self.c_right[1],
        };
        let mut result = [x.map(input[0]), y.map(input[1])];
        if eye == 1 {
            result[0] = 1.0 - result[0];
        }
        result
    }

    /// Per-eye encoded resolution, as used for the MediaCodec width/height.
    /// The encoded side-by-side frame is `encoded_resolution[0] * 2` wide.
    pub fn encoded_view_resolution(&self) -> [u32; 2] {
        self.encoded_resolution
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, epsilon: f32) -> bool {
        (a - b).abs() <= epsilon
    }

    #[test]
    fn default_config_reduces_encoded_resolution() {
        let encoding = FoveatedEncoding::new([1088, 960], FoveatedConfig::default());
        assert!(encoding.encoded_resolution[0] < 1088);
        assert!(encoding.encoded_resolution[1] < 960);
        assert!(encoding.encoded_resolution[0] >= 32);
        assert!(encoding.encoded_resolution[1] >= 32);
    }

    #[test]
    fn mapping_is_monotonic_on_each_axis() {
        let encoding = FoveatedEncoding::new([1088, 960], FoveatedConfig::default());
        let mut previous = -1.0;
        for step in 0..=100 {
            let value = step as f32 / 100.0;
            let mapped = encoding.map_eye_uv(0, [value, 0.5])[0];
            assert!(
                mapped + 1e-4 >= previous,
                "non-monotonic at {value}: {mapped} < {previous}"
            );
            previous = mapped;
        }
    }

    #[test]
    fn edge_ratio_one_is_linear_scale() {
        let config = FoveatedConfig {
            center_size_x: 0.5,
            center_size_y: 0.5,
            center_shift_x: 0.0,
            center_shift_y: 0.0,
            edge_ratio_x: 1.0,
            edge_ratio_y: 1.0,
        };
        let encoding = FoveatedEncoding::new([1024, 1024], config);
        for value in [0.1, 0.25, 0.5, 0.75, 0.9] {
            let mapped = encoding.map_eye_uv(0, [value, value]);
            assert!(approx(mapped[0], value * encoding.view_ratio[0], 1e-4));
            assert!(approx(mapped[1], value * encoding.view_ratio[1], 1e-4));
        }
    }

    #[test]
    fn full_center_region_is_linear() {
        let config = FoveatedConfig {
            center_size_x: 1.0,
            center_size_y: 1.0,
            center_shift_x: 0.0,
            center_shift_y: 0.0,
            edge_ratio_x: 4.0,
            edge_ratio_y: 5.0,
        };
        let encoding = FoveatedEncoding::new([1024, 1024], config);
        let mapped = encoding.map_eye_uv(0, [0.25, 0.75]);
        assert!(approx(mapped[0], 0.25 * encoding.view_ratio[0], 1e-4));
        assert!(approx(mapped[1], 0.75 * encoding.view_ratio[1], 1e-4));
    }

    #[test]
    fn result_stays_in_texture_bounds() {
        let encoding = FoveatedEncoding::new([1088, 960], FoveatedConfig::default());
        for eye in 0..2 {
            for u in [0.0, 0.1, 0.5, 0.9, 1.0] {
                for v in [0.0, 0.1, 0.5, 0.9, 1.0] {
                    let mapped = encoding.map_eye_uv(eye, [u, v]);
                    assert!((0.0..=1.0).contains(&mapped[0]), "{eye} {u} {v} {mapped:?}");
                    assert!((0.0..=1.0).contains(&mapped[1]), "{eye} {u} {v} {mapped:?}");
                }
            }
        }
    }

    #[test]
    fn right_eye_is_mirrored() {
        let encoding = FoveatedEncoding::new([1088, 960], FoveatedConfig::default());
        for uv in [[0.1, 0.2], [0.4, 0.7], [0.9, 0.3]] {
            let left = encoding.map_eye_uv(0, uv);
            let right = encoding.map_eye_uv(1, [1.0 - uv[0], uv[1]]);
            assert!(approx(left[0], 1.0 - right[0], 1e-4), "{uv:?}");
            assert!(approx(left[1], right[1], 1e-4), "{uv:?}");
        }
    }

    #[test]
    fn malformed_config_is_sanitized() {
        let config = FoveatedConfig {
            center_size_x: f32::NAN,
            center_size_y: 99.0,
            center_shift_x: f32::INFINITY,
            center_shift_y: -99.0,
            edge_ratio_x: 0.0,
            edge_ratio_y: 100.0,
        };
        let encoding = FoveatedEncoding::new([1088, 960], config);
        assert!(encoding.encoded_resolution[0] > 0);
        assert!(encoding.encoded_resolution[1] > 0);
        for value in [0.0, 0.5, 1.0] {
            let mapped = encoding.map_eye_uv(0, [value, value]);
            assert!(mapped[0].is_finite());
            assert!(mapped[1].is_finite());
        }
    }
}
