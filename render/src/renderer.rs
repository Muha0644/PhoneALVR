//! Stereo renderer: one distortion mesh, one texture per eye.
//!
//! Steady-state `render()` performs no heap allocation, no locking and no
//! blocking calls. Meshes and shader programs are created on profile/surface
//! changes only.

use glow::HasContext;
use phonealvr_viewer::Mesh;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RenderError {
    #[error("shader compilation failed: {0}")]
    Shader(String),
    #[error("shader program link failed: {0}")]
    Program(String),
    #[error("invalid external buffer")]
    InvalidExternalBuffer,
    #[error("external texture error: {0}")]
    ExternalTexture(&'static str),
    #[error("no distortion mesh has been uploaded")]
    NoMesh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureKind {
    /// Regular `GL_TEXTURE_2D`, used by tests and the lobby/placeholder path.
    Rgba,
    /// `GL_TEXTURE_EXTERNAL_OES`, used for decoded `AHardwareBuffer` frames.
    External,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameParams {
    pub brightness: f32,
    pub vignette: f32,
    pub chromatic_aberration: f32,
    /// Approximate late-latching offset in eye-local UV.
    pub reprojection: [f32; 2],
    pub background: [f32; 4],
}

impl Default for FrameParams {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            vignette: 0.0,
            chromatic_aberration: 0.0,
            reprojection: [0.0, 0.0],
            background: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

impl FrameParams {
    fn sanitized(mut self) -> Self {
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
        self.reprojection = [
            if self.reprojection[0].is_finite() {
                self.reprojection[0].clamp(-0.25, 0.25)
            } else {
                0.0
            },
            if self.reprojection[1].is_finite() {
                self.reprojection[1].clamp(-0.25, 0.25)
            } else {
                0.0
            },
        ];
        self
    }
}

const EXTERNAL_OES: u32 = 0x8D65;

const VERTEX_SHADER: &str = r#"#version 300 es
layout(location = 0) in vec2 a_position;
layout(location = 1) in vec2 a_texcoord;
out vec2 v_texcoord;
void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
    v_texcoord = a_texcoord;
}
"#;

const FRAGMENT_SHADER_2D: &str = r#"#version 300 es
precision highp float;
uniform sampler2D u_texture;
uniform float u_brightness;
uniform float u_vignette;
uniform float u_chromatic_aberration;
uniform vec2 u_uv_scale;
uniform vec2 u_uv_offset;
uniform vec2 u_reprojection;
in vec2 v_texcoord;
out vec4 out_color;

vec2 to_source(vec2 eye_uv) {
    // ALVR encodes the two eyes side by side. The distortion mesh produced
    // eye-local UVs; this maps them to this eye's half of the source frame.
    // `u_reprojection` is the late-latching shift from the newest IMU sample.
    return clamp(eye_uv + u_reprojection, 0.0, 1.0) * u_uv_scale + u_uv_offset;
}

vec3 sample_rgb(vec2 eye_uv) {
    vec2 delta = eye_uv - vec2(0.5);
    if (u_chromatic_aberration > 0.0) {
        vec3 rgb;
        rgb.r = texture(u_texture, to_source(eye_uv + delta * u_chromatic_aberration)).r;
        rgb.g = texture(u_texture, to_source(eye_uv)).g;
        rgb.b = texture(u_texture, to_source(eye_uv - delta * u_chromatic_aberration)).b;
        return rgb;
    }
    return texture(u_texture, to_source(eye_uv)).rgb;
}

void main() {
    vec3 color = sample_rgb(v_texcoord);
    float radius = length(v_texcoord - vec2(0.5)) * 1.41421356;
    float vignette = 1.0 - u_vignette * smoothstep(0.2, 1.0, radius);
    out_color = vec4(color * u_brightness * vignette, 1.0);
}
"#;

#[allow(dead_code)]
const FRAGMENT_SHADER_EXTERNAL: &str = r#"#version 300 es
#extension GL_OES_EGL_image_external_essl3 : require
precision highp float;
uniform samplerExternalOES u_texture;
uniform float u_brightness;
uniform float u_vignette;
uniform float u_chromatic_aberration;
uniform vec2 u_uv_scale;
uniform vec2 u_uv_offset;
uniform vec2 u_reprojection;
in vec2 v_texcoord;
out vec4 out_color;

vec2 to_source(vec2 eye_uv) {
    // ALVR encodes the two eyes side by side. The distortion mesh produced
    // eye-local UVs; this maps them to this eye's half of the source frame.
    // `u_reprojection` is the late-latching shift from the newest IMU sample.
    return clamp(eye_uv + u_reprojection, 0.0, 1.0) * u_uv_scale + u_uv_offset;
}

vec3 sample_rgb(vec2 eye_uv) {
    vec2 delta = eye_uv - vec2(0.5);
    if (u_chromatic_aberration > 0.0) {
        vec3 rgb;
        rgb.r = texture(u_texture, to_source(eye_uv + delta * u_chromatic_aberration)).r;
        rgb.g = texture(u_texture, to_source(eye_uv)).g;
        rgb.b = texture(u_texture, to_source(eye_uv - delta * u_chromatic_aberration)).b;
        return rgb;
    }
    return texture(u_texture, to_source(eye_uv)).rgb;
}

void main() {
    vec3 color = sample_rgb(v_texcoord);
    float radius = length(v_texcoord - vec2(0.5)) * 1.41421356;
    float vignette = 1.0 - u_vignette * smoothstep(0.2, 1.0, radius);
    out_color = vec4(color * u_brightness * vignette, 1.0);
}
"#;

struct Program {
    program: glow::Program,
    u_texture: Option<glow::UniformLocation>,
    u_brightness: Option<glow::UniformLocation>,
    u_vignette: Option<glow::UniformLocation>,
    u_chromatic_aberration: Option<glow::UniformLocation>,
    u_uv_scale: Option<glow::UniformLocation>,
    u_uv_offset: Option<glow::UniformLocation>,
    u_reprojection: Option<glow::UniformLocation>,
}

impl Program {
    fn new(gl: &glow::Context, fragment_source: &str) -> Result<Self, RenderError> {
        unsafe {
            let vertex = compile(gl, glow::VERTEX_SHADER, VERTEX_SHADER)?;
            let fragment = compile(gl, glow::FRAGMENT_SHADER, fragment_source)?;
            let program = gl
                .create_program()
                .map_err(|_| RenderError::Program("glCreateProgram failed".into()))?;
            gl.attach_shader(program, vertex);
            gl.attach_shader(program, fragment);
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                let log = gl.get_program_info_log(program);
                gl.delete_shader(vertex);
                gl.delete_shader(fragment);
                gl.delete_program(program);
                return Err(RenderError::Program(log));
            }
            gl.detach_shader(program, vertex);
            gl.detach_shader(program, fragment);
            gl.delete_shader(vertex);
            gl.delete_shader(fragment);

            Ok(Self {
                program,
                u_texture: gl.get_uniform_location(program, "u_texture"),
                u_brightness: gl.get_uniform_location(program, "u_brightness"),
                u_vignette: gl.get_uniform_location(program, "u_vignette"),
                u_chromatic_aberration: gl.get_uniform_location(program, "u_chromatic_aberration"),
                u_uv_scale: gl.get_uniform_location(program, "u_uv_scale"),
                u_uv_offset: gl.get_uniform_location(program, "u_uv_offset"),
                u_reprojection: gl.get_uniform_location(program, "u_reprojection"),
            })
        }
    }

    unsafe fn use_program(&self, gl: &glow::Context) {
        gl.use_program(Some(self.program));
    }

    unsafe fn set_params(
        &self,
        gl: &glow::Context,
        params: FrameParams,
        texture_unit: i32,
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
    ) {
        gl.uniform_1_i32(self.u_texture.as_ref(), texture_unit);
        gl.uniform_1_f32(self.u_brightness.as_ref(), params.brightness);
        gl.uniform_1_f32(self.u_vignette.as_ref(), params.vignette);
        gl.uniform_1_f32(
            self.u_chromatic_aberration.as_ref(),
            params.chromatic_aberration,
        );
        gl.uniform_2_f32(self.u_uv_scale.as_ref(), uv_scale[0], uv_scale[1]);
        gl.uniform_2_f32(self.u_uv_offset.as_ref(), uv_offset[0], uv_offset[1]);
        gl.uniform_2_f32(
            self.u_reprojection.as_ref(),
            params.reprojection[0],
            params.reprojection[1],
        );
    }
}

unsafe fn compile(
    gl: &glow::Context,
    shader_type: u32,
    source: &str,
) -> Result<glow::Shader, RenderError> {
    let shader = gl
        .create_shader(shader_type)
        .map_err(|_| RenderError::Program("glCreateShader failed".into()))?;
    gl.shader_source(shader, source);
    gl.compile_shader(shader);
    if !gl.get_shader_compile_status(shader) {
        let log = gl.get_shader_info_log(shader);
        gl.delete_shader(shader);
        return Err(RenderError::Shader(log));
    }
    Ok(shader)
}

pub struct StereoRenderer {
    program_2d: Program,
    program_external: Option<Program>,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    index_count: i32,
    width: i32,
    height: i32,
}

impl StereoRenderer {
    pub fn new(gl: &glow::Context) -> Result<Self, RenderError> {
        let program_2d = Program::new(gl, FRAGMENT_SHADER_2D)?;
        #[cfg(target_os = "android")]
        let program_external = Some(Program::new(gl, FRAGMENT_SHADER_EXTERNAL)?);
        #[cfg(not(target_os = "android"))]
        let program_external = None;

        unsafe {
            let vao = gl
                .create_vertex_array()
                .map_err(|_| RenderError::Program("glGenVertexArrays failed".into()))?;
            let vbo = gl
                .create_buffer()
                .map_err(|_| RenderError::Program("glGenBuffers failed".into()))?;
            let ebo = gl
                .create_buffer()
                .map_err(|_| RenderError::Program("glGenBuffers failed".into()))?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 16, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 16, 8);
            gl.bind_vertex_array(None);

            Ok(Self {
                program_2d,
                program_external,
                vao,
                vbo,
                ebo,
                index_count: 0,
                width: 1,
                height: 1,
            })
        }
    }

    pub fn resize(&mut self, width: i32, height: i32) {
        self.width = width.max(1);
        self.height = height.max(1);
    }

    /// Upload a new distortion mesh. Called on profile or display changes.
    pub fn upload_mesh(&mut self, gl: &glow::Context, mesh: &Mesh) -> Result<(), RenderError> {
        let mut vertices = Vec::with_capacity(mesh.positions.len() * 4);
        for (position, uv) in mesh.positions.iter().zip(mesh.tex_coords.iter()) {
            vertices.extend_from_slice(&[position[0], position[1], uv[0], uv[1]]);
        }
        let vertex_bytes = unsafe {
            std::slice::from_raw_parts(
                vertices.as_ptr() as *const u8,
                vertices.len() * std::mem::size_of::<f32>(),
            )
        };
        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                mesh.indices.as_ptr() as *const u8,
                mesh.indices.len() * std::mem::size_of::<u32>(),
            )
        };

        unsafe {
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::STATIC_DRAW);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ebo));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, index_bytes, glow::STATIC_DRAW);
            gl.bind_vertex_array(None);
        }

        self.index_count = mesh.indices.len() as i32;
        Ok(())
    }

    /// Draw both eyes. `textures` are GL texture names, left then right.
    ///
    /// # Safety
    /// The caller must have made a GLES context current on this thread and
    /// must pass textures created in that same context. `self` must have been
    /// created with the same context.
    pub unsafe fn render(
        &self,
        gl: &glow::Context,
        textures: [glow::Texture; 2],
        kind: TextureKind,
        params: FrameParams,
    ) -> Result<(), RenderError> {
        if self.index_count == 0 {
            return Err(RenderError::NoMesh);
        }
        let params = params.sanitized();

        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::BLEND);
        gl.clear_color(
            params.background[0],
            params.background[1],
            params.background[2],
            params.background[3],
        );
        gl.clear(glow::COLOR_BUFFER_BIT);
        gl.bind_vertex_array(Some(self.vao));

        let eye_width = (self.width / 2).max(1);
        let program = match kind {
            TextureKind::External => self.program_external.as_ref().unwrap_or(&self.program_2d),
            TextureKind::Rgba => &self.program_2d,
        };

        for (eye, texture) in textures.iter().enumerate() {
            let (uv_scale, uv_offset) = side_by_side_uv(eye);
            let x = eye as i32 * eye_width;
            gl.viewport(x, 0, eye_width, self.height);
            gl.active_texture(glow::TEXTURE0);
            match kind {
                TextureKind::External => gl.bind_texture(EXTERNAL_OES, Some(*texture)),
                TextureKind::Rgba => gl.bind_texture(glow::TEXTURE_2D, Some(*texture)),
            }
            program.use_program(gl);
            program.set_params(gl, params, 0, uv_scale, uv_offset);
            gl.draw_elements(glow::TRIANGLES, self.index_count, glow::UNSIGNED_INT, 0);
        }

        gl.bind_vertex_array(None);
        Ok(())
    }

    /// Delete GL objects while the context is current.
    ///
    /// # Safety
    /// A GLES context shared with this renderer must be current on the calling
    /// thread. Calling this from another context is undefined behaviour, and
    /// `self` must not be used afterwards.
    pub unsafe fn destroy(&mut self, gl: &glow::Context) {
        gl.delete_program(self.program_2d.program);
        if let Some(program) = &self.program_external {
            gl.delete_program(program.program);
        }
        gl.delete_vertex_array(self.vao);
        gl.delete_buffer(self.vbo);
        gl.delete_buffer(self.ebo);
        self.index_count = 0;
    }
}

/// ALVR's video texture contains the left and right eyes side by side.
/// The distortion mesh emits eye-local UVs; select this eye's half here.
fn side_by_side_uv(eye: usize) -> ([f32; 2], [f32; 2]) {
    let scale = [0.5, 1.0];
    let offset = [0.5 * eye as f32, 0.0];
    (scale, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaders_are_gles3() {
        assert!(VERTEX_SHADER.starts_with("#version 300 es"));
        assert!(FRAGMENT_SHADER_2D.starts_with("#version 300 es"));
        assert!(FRAGMENT_SHADER_EXTERNAL.starts_with("#version 300 es"));
        assert!(FRAGMENT_SHADER_EXTERNAL.contains("samplerExternalOES"));
    }

    #[test]
    fn side_by_side_uv_selects_each_half() {
        let (scale, offset) = side_by_side_uv(0);
        assert_eq!(scale, [0.5, 1.0]);
        assert_eq!(offset, [0.0, 0.0]);
        let (scale, offset) = side_by_side_uv(1);
        assert_eq!(scale, [0.5, 1.0]);
        assert_eq!(offset, [0.5, 0.0]);
    }

    #[test]
    fn frame_params_are_sanitized() {
        let params = FrameParams {
            brightness: 99.0,
            vignette: f32::NAN,
            chromatic_aberration: -1.0,
            reprojection: [f32::NAN, 9.0],
            background: [0.0; 4],
        }
        .sanitized();
        assert_eq!(params.brightness, 1.0);
        assert_eq!(params.vignette, 0.0);
        assert_eq!(params.chromatic_aberration, 0.0);
        assert_eq!(params.reprojection, [0.0, 0.25]);
    }
}
