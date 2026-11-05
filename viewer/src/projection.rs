//! Minimal column-major matrix helpers for the stereo renderer.
//!
//! We keep this small instead of pulling a full linear-algebra crate into the
//! render path. The conventions match OpenGL ES: right-handed world, camera
//! looking down -Z, clip space -1..1, column-major storage.

use crate::profile::EyeFov;

pub type Mat4 = [f32; 16];

pub fn identity() -> Mat4 {
    let mut matrix = [0.0; 16];
    matrix[0] = 1.0;
    matrix[5] = 1.0;
    matrix[10] = 1.0;
    matrix[15] = 1.0;
    matrix
}

/// Asymmetric perspective projection from an eye's tangent-space FOV.
pub fn projection_matrix(fov: EyeFov, near: f32, far: f32) -> Mat4 {
    let near = near.max(1e-4);
    let far = far.max(near + 1e-3);
    let tan = fov.tan_extents();

    let left = tan.left * near;
    let right = tan.right * near;
    let bottom = tan.bottom * near;
    let top = tan.top * near;

    let width = (right - left).max(1e-6);
    let height = (top - bottom).max(1e-6);
    let depth = far - near;

    let mut matrix = [0.0; 16];
    matrix[0] = 2.0 * near / width;
    matrix[5] = 2.0 * near / height;
    matrix[8] = (right + left) / width;
    matrix[9] = (top + bottom) / height;
    matrix[10] = -(far + near) / depth;
    matrix[11] = -1.0;
    matrix[14] = -(2.0 * far * near) / depth;
    matrix
}

/// View rotation from a unit quaternion `[x, y, z, w]` describing the head
/// orientation in world space. The returned matrix transforms world-space
/// vectors into head space (the inverse rotation).
pub fn view_rotation_matrix(orientation: [f32; 4]) -> Mat4 {
    let [mut x, mut y, mut z, mut w] = orientation;
    let length = (x * x + y * y + z * z + w * w).sqrt();
    if length <= f32::EPSILON || !length.is_finite() {
        x = 0.0;
        y = 0.0;
        z = 0.0;
        w = 1.0;
    } else {
        x /= length;
        y /= length;
        z /= length;
        w /= length;
    }

    // Rotation matrix for the inverse rotation (transpose of camera-to-world).
    let mut matrix = identity();
    matrix[0] = 1.0 - 2.0 * (y * y + z * z);
    matrix[1] = 2.0 * (x * y - z * w);
    matrix[2] = 2.0 * (x * z + y * w);
    matrix[4] = 2.0 * (x * y + z * w);
    matrix[5] = 1.0 - 2.0 * (x * x + z * z);
    matrix[6] = 2.0 * (y * z - x * w);
    matrix[8] = 2.0 * (x * z - y * w);
    matrix[9] = 2.0 * (y * z + x * w);
    matrix[10] = 1.0 - 2.0 * (x * x + y * y);
    matrix
}

/// Multiply two column-major matrices (`lhs * rhs`).
pub fn multiply(lhs: Mat4, rhs: Mat4) -> Mat4 {
    let mut result = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += lhs[k * 4 + row] * rhs[col * 4 + k];
            }
            result[col * 4 + row] = sum;
        }
    }
    result
}

/// Transform a point by a column-major matrix and return NDC coordinates.
pub fn transform_point(matrix: Mat4, point: [f32; 3]) -> [f32; 3] {
    let x = matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12];
    let y = matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13];
    let z = matrix[2] * point[0] + matrix[6] * point[1] + matrix[10] * point[2] + matrix[14];
    let w = matrix[3] * point[0] + matrix[7] * point[1] + matrix[11] * point[2] + matrix[15];
    if w.abs() <= f32::EPSILON {
        [f32::NAN; 3]
    } else {
        [x / w, y / w, z / w]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(left: f32, right: f32, epsilon: f32) -> bool {
        (left - right).abs() <= epsilon
    }

    #[test]
    fn identity_is_identity() {
        let matrix = identity();
        assert_eq!(matrix[0], 1.0);
        assert_eq!(matrix[5], 1.0);
        assert_eq!(matrix[10], 1.0);
        assert_eq!(matrix[15], 1.0);
        assert_eq!(matrix[1], 0.0);
    }

    #[test]
    fn centre_of_view_maps_to_ndc_origin() {
        let fov = EyeFov::new(50.0, 50.0, 40.0, 40.0);
        let projection = projection_matrix(fov, 0.1, 100.0);
        let ndc = transform_point(projection, [0.0, 0.0, -1.0]);
        assert!(approx(ndc[0], 0.0, 1e-5));
        assert!(approx(ndc[1], 0.0, 1e-5));
        assert!(ndc[2] > -1.0 && ndc[2] < 1.0);
    }

    #[test]
    fn fov_edges_map_to_clip_edges() {
        let fov = EyeFov::new(50.0, 50.0, 40.0, 40.0);
        let projection = projection_matrix(fov, 0.1, 100.0);
        let tan = fov.tan_extents();
        let right_edge = transform_point(projection, [tan.right, 0.0, -1.0]);
        let left_edge = transform_point(projection, [tan.left, 0.0, -1.0]);
        let top_edge = transform_point(projection, [0.0, tan.top, -1.0]);
        let bottom_edge = transform_point(projection, [0.0, tan.bottom, -1.0]);
        assert!(approx(right_edge[0], 1.0, 1e-5));
        assert!(approx(left_edge[0], -1.0, 1e-5));
        assert!(approx(top_edge[1], 1.0, 1e-5));
        assert!(approx(bottom_edge[1], -1.0, 1e-5));
    }

    #[test]
    fn projection_handles_invalid_near_far() {
        let fov = EyeFov::symmetric(90.0);
        let projection = projection_matrix(fov, 0.0, 0.0);
        assert!(projection.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn identity_view_rotation_keeps_axes() {
        let matrix = view_rotation_matrix([0.0, 0.0, 0.0, 1.0]);
        let point = [1.0, 2.0, 3.0];
        let transformed = transform_point(matrix, point);
        assert!(approx(transformed[0], 1.0, 1e-5));
        assert!(approx(transformed[1], 2.0, 1e-5));
        assert!(approx(transformed[2], 3.0, 1e-5));
    }

    #[test]
    fn yaw_ninety_maps_world_x_to_positive_local_z() {
        let half = std::f32::consts::FRAC_PI_4;
        let matrix = view_rotation_matrix([0.0, half.sin(), 0.0, half.cos()]);
        let transformed = transform_point(matrix, [1.0, 0.0, 0.0]);
        assert!(approx(transformed[0], 0.0, 1e-5));
        assert!(approx(transformed[2], 1.0, 1e-5));
    }
}
