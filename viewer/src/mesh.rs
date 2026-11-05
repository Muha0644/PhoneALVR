//! Precomputed distortion meshes.
//!
//! A mesh is generated whenever the viewer profile or display size changes,
//! never per frame. It is intentionally tiny: a 32x24 grid is enough for
//! smooth barrel distortion at phone resolutions, and the fragment shader can
//! add chromatic aberration by offsetting the sampled UV slightly per channel.

use crate::distortion::EyeMapper;

#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// Normalised device coordinates for one eye viewport: -1..1 on both axes,
    /// y up.
    pub positions: Vec<[f32; 2]>,
    pub tex_coords: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub cols: u32,
    pub rows: u32,
}

impl Mesh {
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn index_count(&self) -> usize {
        self.indices.len()
    }
}

/// Build a triangle mesh for one eye. `cols` and `rows` are the number of
/// quads, clamped to a safe range so a malformed profile cannot allocate an
/// unbounded buffer.
pub fn build_eye_mesh(mapper: &EyeMapper, cols: u32, rows: u32) -> Mesh {
    let cols = cols.clamp(2, 128);
    let rows = rows.clamp(2, 128);
    let vertex_cols = cols + 1;
    let vertex_rows = rows + 1;
    let vertex_count = (vertex_cols * vertex_rows) as usize;

    let mut positions = Vec::with_capacity(vertex_count);
    let mut tex_coords = Vec::with_capacity(vertex_count);

    for row in 0..vertex_rows {
        let v = row as f32 / rows as f32;
        for col in 0..vertex_cols {
            let u = col as f32 / cols as f32;
            positions.push([u * 2.0 - 1.0, 1.0 - v * 2.0]);
            tex_coords.push(mapper.source_uv([u, v]));
        }
    }

    let index_count = (cols * rows * 6) as usize;
    let mut indices = Vec::with_capacity(index_count);
    for row in 0..rows {
        for col in 0..cols {
            let top_left = row * vertex_cols + col;
            let top_right = top_left + 1;
            let bottom_left = (row + 1) * vertex_cols + col;
            let bottom_right = bottom_left + 1;

            // Two triangles, counter-clockwise when viewed from +z.
            indices.extend_from_slice(&[top_left, bottom_left, top_right]);
            indices.extend_from_slice(&[top_right, bottom_left, bottom_right]);
        }
    }

    Mesh {
        positions,
        tex_coords,
        indices,
        cols,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::ViewerProfile;

    #[test]
    fn mesh_has_expected_topology() {
        let profile = ViewerProfile::cardboard_v1();
        let mapper = EyeMapper::from_profile(&profile);
        let mesh = build_eye_mesh(&mapper, 8, 6);
        assert_eq!(mesh.vertex_count(), 9 * 7);
        assert_eq!(mesh.index_count(), 8 * 6 * 6);
        assert_eq!(mesh.positions.len(), mesh.tex_coords.len());
        assert!(mesh
            .indices
            .iter()
            .all(|index| (*index as usize) < mesh.vertex_count()));
    }

    #[test]
    fn grid_is_clamped() {
        let profile = ViewerProfile::cardboard_v1();
        let mapper = EyeMapper::from_profile(&profile);
        assert_eq!(build_eye_mesh(&mapper, 0, 0).vertex_count(), 3 * 3);
        assert_eq!(
            build_eye_mesh(&mapper, 10_000, 10_000).vertex_count(),
            129 * 129
        );
    }

    #[test]
    fn corners_are_ndc_corners() {
        let profile = ViewerProfile::cardboard_v1();
        let mapper = EyeMapper::from_profile(&profile);
        let mesh = build_eye_mesh(&mapper, 4, 4);
        assert_eq!(mesh.positions[0], [-1.0, 1.0]);
        assert_eq!(mesh.positions[4], [1.0, 1.0]);
        assert_eq!(mesh.positions[mesh.positions.len() - 5], [-1.0, -1.0]);
        assert_eq!(*mesh.positions.last().unwrap(), [1.0, -1.0]);
    }

    #[test]
    fn centre_vertex_maps_to_centre() {
        let profile = ViewerProfile::cardboard_v1();
        let mapper = EyeMapper::from_profile(&profile);
        let mesh = build_eye_mesh(&mapper, 4, 4);
        let index = 2 * 5 + 2;
        assert!((mesh.tex_coords[index][0] - 0.5).abs() < 1e-5);
        assert!((mesh.tex_coords[index][1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn indices_form_two_triangles_per_quad() {
        let profile = ViewerProfile::cardboard_v1();
        let mapper = EyeMapper::from_profile(&profile);
        let mesh = build_eye_mesh(&mapper, 2, 2);
        assert_eq!(&mesh.indices[0..6], &[0, 3, 1, 1, 3, 4]);
    }
}
