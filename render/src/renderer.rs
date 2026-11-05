//! Stereo renderer: one distortion mesh, one texture per eye.
//!
//! Steady-state `render()` performs no heap allocation, no locking and no
//! blocking calls. Meshes and shader programs are created on profile/surface
//! changes only.

use glow::HasContext;
use phonealvr_viewer::{FoveatedEncoding, Mesh};

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

/// Per-stream foveated-encoding shader constants.
///
/// These mirror `phonealvr_viewer::FoveatedEncoding`; they only change when a
/// new stream or FFE configuration arrives, never per frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoveatedShaderParams {
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

impl FoveatedShaderParams {
    fn is_finite(&self) -> bool {
        [
            self.view_ratio,
            self.edge_ratio,
            self.c1,
            self.c2,
            self.lo_bound,
            self.hi_bound,
            self.a_left,
            self.b_left,
            self.a_right,
            self.b_right,
            self.c_right,
        ]
        .iter()
        .flatten()
        .all(|value| value.is_finite())
    }
}

impl From<FoveatedEncoding> for FoveatedShaderParams {
    fn from(encoding: FoveatedEncoding) -> Self {
        Self {
            view_ratio: encoding.view_ratio,
            edge_ratio: encoding.edge_ratio,
            c1: encoding.c1,
            c2: encoding.c2,
            lo_bound: encoding.lo_bound,
            hi_bound: encoding.hi_bound,
            a_left: encoding.a_left,
            b_left: encoding.b_left,
            a_right: encoding.a_right,
            b_right: encoding.b_right,
            c_right: encoding.c_right,
        }
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
uniform int u_ffe_enabled;
uniform int u_eye_index;
uniform vec2 u_ffe_view_ratio;
uniform vec2 u_ffe_edge_ratio;
uniform vec2 u_ffe_c1;
uniform vec2 u_ffe_c2;
uniform vec2 u_ffe_lo_bound;
uniform vec2 u_ffe_hi_bound;
uniform vec2 u_ffe_a_left;
uniform vec2 u_ffe_b_left;
uniform vec2 u_ffe_a_right;
uniform vec2 u_ffe_b_right;
uniform vec2 u_ffe_c_right;
in vec2 v_texcoord;
out vec4 out_color;

vec2 ffe_map(vec2 uv) {
    // ALVR's foveated layout keeps a sharp centre band and compresses the
    // edges. Invert that here, after lens distortion and before the
    // side-by-side split. Mirrors `viewer::FoveatedEncoding::map_eye_uv`.
    if (u_eye_index == 1) {
        uv.x = 1.0 - uv.x;
    }
    vec2 center = (uv - u_ffe_c1) * u_ffe_edge_ratio / u_ffe_c2;

    // `a_left`/`a_right` are zero for a linear (edge_ratio == 1) layout; fall
    // back to the identity edge in that case instead of dividing by zero.
    vec2 left_scale = 2.0 * u_ffe_a_left;
    vec2 right_scale = 2.0 * u_ffe_a_right;
    // 1.0 where the coefficient is usable, 0.0 where it is ~zero.
    vec2 left_valid = step(vec2(1e-5), abs(left_scale));
    vec2 right_valid = step(vec2(1e-5), abs(right_scale));
    vec2 left_divisor = left_scale + (1.0 - left_valid);
    vec2 right_divisor = right_scale + (1.0 - right_valid);

    vec2 left_discriminant = max(
        u_ffe_b_left * u_ffe_b_left + 4.0 * u_ffe_a_left * uv,
        vec2(0.0)
    );
    vec2 right_discriminant = max(
        u_ffe_b_right * u_ffe_b_right
            - 4.0 * (u_ffe_c_right - u_ffe_a_right * uv),
        vec2(0.0)
    );
    vec2 left_edge = mix(uv, (-u_ffe_b_left + sqrt(left_discriminant)) / left_divisor, left_valid);
    vec2 right_edge = mix(uv, (-u_ffe_b_right + sqrt(right_discriminant)) / right_divisor, right_valid);

    vec2 mapped = center;
    mapped.x = uv.x < u_ffe_lo_bound.x ? left_edge.x
        : (uv.x > u_ffe_hi_bound.x ? right_edge.x : center.x);
    mapped.y = uv.y < u_ffe_lo_bound.y ? left_edge.y
        : (uv.y > u_ffe_hi_bound.y ? right_edge.y : center.y);
    mapped = mapped * u_ffe_view_ratio;

    if (u_eye_index == 1) {
        mapped.x = 1.0 - mapped.x;
    }
    return clamp(mapped, 0.0, 1.0);
}

vec2 to_source(vec2 eye_uv) {
    // ALVR encodes the two eyes side by side. The distortion mesh produced
    // eye-local UVs; undo the foveated layout, then map them to this eye's
    // half of the source frame. `u_reprojection` is the late-latching shift
    // from the newest IMU sample.
    vec2 uv = clamp(eye_uv + u_reprojection, 0.0, 1.0);
    if (u_ffe_enabled != 0) {
        uv = ffe_map(uv);
    }
    return uv * u_uv_scale + u_uv_offset;
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
uniform int u_ffe_enabled;
uniform int u_eye_index;
uniform vec2 u_ffe_view_ratio;
uniform vec2 u_ffe_edge_ratio;
uniform vec2 u_ffe_c1;
uniform vec2 u_ffe_c2;
uniform vec2 u_ffe_lo_bound;
uniform vec2 u_ffe_hi_bound;
uniform vec2 u_ffe_a_left;
uniform vec2 u_ffe_b_left;
uniform vec2 u_ffe_a_right;
uniform vec2 u_ffe_b_right;
uniform vec2 u_ffe_c_right;
in vec2 v_texcoord;
out vec4 out_color;

vec2 ffe_map(vec2 uv) {
    // ALVR's foveated layout keeps a sharp centre band and compresses the
    // edges. Invert that here, after lens distortion and before the
    // side-by-side split. Mirrors `viewer::FoveatedEncoding::map_eye_uv`.
    if (u_eye_index == 1) {
        uv.x = 1.0 - uv.x;
    }
    vec2 center = (uv - u_ffe_c1) * u_ffe_edge_ratio / u_ffe_c2;

    // `a_left`/`a_right` are zero for a linear (edge_ratio == 1) layout; fall
    // back to the identity edge in that case instead of dividing by zero.
    vec2 left_scale = 2.0 * u_ffe_a_left;
    vec2 right_scale = 2.0 * u_ffe_a_right;
    // 1.0 where the coefficient is usable, 0.0 where it is ~zero.
    vec2 left_valid = step(vec2(1e-5), abs(left_scale));
    vec2 right_valid = step(vec2(1e-5), abs(right_scale));
    vec2 left_divisor = left_scale + (1.0 - left_valid);
    vec2 right_divisor = right_scale + (1.0 - right_valid);

    vec2 left_discriminant = max(
        u_ffe_b_left * u_ffe_b_left + 4.0 * u_ffe_a_left * uv,
        vec2(0.0)
    );
    vec2 right_discriminant = max(
        u_ffe_b_right * u_ffe_b_right
            - 4.0 * (u_ffe_c_right - u_ffe_a_right * uv),
        vec2(0.0)
    );
    vec2 left_edge = mix(uv, (-u_ffe_b_left + sqrt(left_discriminant)) / left_divisor, left_valid);
    vec2 right_edge = mix(uv, (-u_ffe_b_right + sqrt(right_discriminant)) / right_divisor, right_valid);

    vec2 mapped = center;
    mapped.x = uv.x < u_ffe_lo_bound.x ? left_edge.x
        : (uv.x > u_ffe_hi_bound.x ? right_edge.x : center.x);
    mapped.y = uv.y < u_ffe_lo_bound.y ? left_edge.y
        : (uv.y > u_ffe_hi_bound.y ? right_edge.y : center.y);
    mapped = mapped * u_ffe_view_ratio;

    if (u_eye_index == 1) {
        mapped.x = 1.0 - mapped.x;
    }
    return clamp(mapped, 0.0, 1.0);
}

vec2 to_source(vec2 eye_uv) {
    // ALVR encodes the two eyes side by side. The distortion mesh produced
    // eye-local UVs; undo the foveated layout, then map them to this eye's
    // half of the source frame. `u_reprojection` is the late-latching shift
    // from the newest IMU sample.
    vec2 uv = clamp(eye_uv + u_reprojection, 0.0, 1.0);
    if (u_ffe_enabled != 0) {
        uv = ffe_map(uv);
    }
    return uv * u_uv_scale + u_uv_offset;
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
    u_ffe_enabled: Option<glow::UniformLocation>,
    u_eye_index: Option<glow::UniformLocation>,
    u_ffe_view_ratio: Option<glow::UniformLocation>,
    u_ffe_edge_ratio: Option<glow::UniformLocation>,
    u_ffe_c1: Option<glow::UniformLocation>,
    u_ffe_c2: Option<glow::UniformLocation>,
    u_ffe_lo_bound: Option<glow::UniformLocation>,
    u_ffe_hi_bound: Option<glow::UniformLocation>,
    u_ffe_a_left: Option<glow::UniformLocation>,
    u_ffe_b_left: Option<glow::UniformLocation>,
    u_ffe_a_right: Option<glow::UniformLocation>,
    u_ffe_b_right: Option<glow::UniformLocation>,
    u_ffe_c_right: Option<glow::UniformLocation>,
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
                u_ffe_enabled: gl.get_uniform_location(program, "u_ffe_enabled"),
                u_eye_index: gl.get_uniform_location(program, "u_eye_index"),
                u_ffe_view_ratio: gl.get_uniform_location(program, "u_ffe_view_ratio"),
                u_ffe_edge_ratio: gl.get_uniform_location(program, "u_ffe_edge_ratio"),
                u_ffe_c1: gl.get_uniform_location(program, "u_ffe_c1"),
                u_ffe_c2: gl.get_uniform_location(program, "u_ffe_c2"),
                u_ffe_lo_bound: gl.get_uniform_location(program, "u_ffe_lo_bound"),
                u_ffe_hi_bound: gl.get_uniform_location(program, "u_ffe_hi_bound"),
                u_ffe_a_left: gl.get_uniform_location(program, "u_ffe_a_left"),
                u_ffe_b_left: gl.get_uniform_location(program, "u_ffe_b_left"),
                u_ffe_a_right: gl.get_uniform_location(program, "u_ffe_a_right"),
                u_ffe_b_right: gl.get_uniform_location(program, "u_ffe_b_right"),
                u_ffe_c_right: gl.get_uniform_location(program, "u_ffe_c_right"),
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
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        foveated: Option<&FoveatedShaderParams>,
        eye: usize,
    ) {
        gl.uniform_1_i32(self.u_texture.as_ref(), 0);
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
        gl.uniform_1_i32(self.u_eye_index.as_ref(), eye as i32);
        match foveated {
            Some(foveated) => {
                gl.uniform_1_i32(self.u_ffe_enabled.as_ref(), 1);
                set_vec2(gl, &self.u_ffe_view_ratio, foveated.view_ratio);
                set_vec2(gl, &self.u_ffe_edge_ratio, foveated.edge_ratio);
                set_vec2(gl, &self.u_ffe_c1, foveated.c1);
                set_vec2(gl, &self.u_ffe_c2, foveated.c2);
                set_vec2(gl, &self.u_ffe_lo_bound, foveated.lo_bound);
                set_vec2(gl, &self.u_ffe_hi_bound, foveated.hi_bound);
                set_vec2(gl, &self.u_ffe_a_left, foveated.a_left);
                set_vec2(gl, &self.u_ffe_b_left, foveated.b_left);
                set_vec2(gl, &self.u_ffe_a_right, foveated.a_right);
                set_vec2(gl, &self.u_ffe_b_right, foveated.b_right);
                set_vec2(gl, &self.u_ffe_c_right, foveated.c_right);
            }
            None => gl.uniform_1_i32(self.u_ffe_enabled.as_ref(), 0),
        }
    }
}

unsafe fn set_vec2(gl: &glow::Context, location: &Option<glow::UniformLocation>, value: [f32; 2]) {
    gl.uniform_2_f32(location.as_ref(), value[0], value[1]);
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
    foveated: Option<FoveatedShaderParams>,
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
                foveated: None,
            })
        }
    }

    pub fn resize(&mut self, width: i32, height: i32) {
        self.width = width.max(1);
        self.height = height.max(1);
    }

    /// Enable or disable the foveated-encoding UV remap. Called once per
    /// stream/FFE configuration change, never per frame. Non-finite constants
    /// (from a malformed server config) disable the remap rather than
    /// producing NaN UVs.
    pub fn set_foveated(&mut self, foveated: Option<FoveatedShaderParams>) {
        match foveated {
            Some(params) if params.is_finite() => self.foveated = Some(params),
            Some(_) => {
                log::warn!("ignoring non-finite foveated-encoding constants");
                self.foveated = None;
            }
            None => self.foveated = None,
        }
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
            program.set_params(gl, params, uv_scale, uv_offset, self.foveated.as_ref(), eye);
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

    #[test]
    fn foveated_shader_params_copy_encoding_constants() {
        use phonealvr_viewer::{FoveatedConfig, FoveatedEncoding};

        let encoding = FoveatedEncoding::new([1088, 960], FoveatedConfig::default());
        let params = FoveatedShaderParams::from(encoding);
        assert_eq!(params.view_ratio, encoding.view_ratio);
        assert_eq!(params.edge_ratio, encoding.edge_ratio);
        assert_eq!(params.c1, encoding.c1);
        assert_eq!(params.c2, encoding.c2);
        assert_eq!(params.lo_bound, encoding.lo_bound);
        assert_eq!(params.hi_bound, encoding.hi_bound);
        assert_eq!(params.a_left, encoding.a_left);
        assert_eq!(params.b_left, encoding.b_left);
        assert_eq!(params.a_right, encoding.a_right);
        assert_eq!(params.b_right, encoding.b_right);
        assert_eq!(params.c_right, encoding.c_right);
        assert!(params.is_finite());
    }

    #[test]
    fn non_finite_foveated_params_are_rejected() {
        let mut params = FoveatedShaderParams {
            view_ratio: [1.0; 2],
            edge_ratio: [1.0; 2],
            c1: [0.0; 2],
            c2: [1.0; 2],
            lo_bound: [0.0; 2],
            hi_bound: [1.0; 2],
            a_left: [0.0; 2],
            b_left: [0.0; 2],
            a_right: [0.0; 2],
            b_right: [0.0; 2],
            c_right: [0.0; 2],
        };
        assert!(params.is_finite());
        params.c2[1] = f32::NAN;
        assert!(!params.is_finite());
    }

    #[test]
    fn both_shaders_apply_ffe_after_distortion_before_side_by_side() {
        for shader in [FRAGMENT_SHADER_2D, FRAGMENT_SHADER_EXTERNAL] {
            assert!(shader.contains("uniform int u_ffe_enabled;"));
            assert!(shader.contains("uniform int u_eye_index;"));
            let distortion_done = shader.find("vec2 to_source").expect("to_source");
            let ffe = shader.find("uv = ffe_map(uv);").expect("ffe call");
            let split = shader
                .find("* u_uv_scale + u_uv_offset")
                .expect("side-by-side split");
            assert!(distortion_done < ffe, "FFE before lens UV");
            assert!(ffe < split, "FFE after lens UV, before side-by-side");
            // Mirror both before and after the FFE transform for the right eye.
            assert_eq!(shader.matches("u_eye_index == 1").count(), 2);
        }
    }
}
