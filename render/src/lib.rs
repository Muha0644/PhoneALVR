//! OpenGL ES 3.0 renderer for the stereo stream and lens distortion.
//!
//! The renderer is deliberately small: one full-screen distortion mesh per eye,
//! one texture per eye, and a fragment shader that applies brightness,
//! vignette and optional chromatic-aberration offset. Mesh generation happens
//! on profile/display changes, never per frame.

pub mod renderer;
pub mod texture;

#[cfg(target_os = "android")]
pub mod egl;

pub use renderer::{FrameParams, RenderError, StereoRenderer, TextureKind};
pub use texture::ExternalTexture;
