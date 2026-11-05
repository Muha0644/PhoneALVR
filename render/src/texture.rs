//! Zero-copy external textures backed by `AHardwareBuffer`.
//!
//! The ALVR decoder hands out an `AHardwareBuffer` per decoded frame. On
//! Android we wrap it in an `EGLImage` and bind that image to a
//! `GL_TEXTURE_EXTERNAL_OES` texture. No pixel data crosses the CPU.
//!
//! The GL texture is persistent (created once per surface); only the EGLImage
//! is recreated per frame, matching ALVR's own staging renderer.

use crate::renderer::RenderError;

#[cfg(target_os = "android")]
pub const EXTERNAL_OES: u32 = 0x8D65;

/// Create the persistent external texture used for the video stream.
#[cfg(target_os = "android")]
pub fn create_external_texture(gl: &glow::Context) -> Option<glow::Texture> {
    use glow::HasContext;
    unsafe {
        let texture = gl.create_texture().ok()?;
        gl.bind_texture(EXTERNAL_OES, Some(texture));
        gl.tex_parameter_i32(
            EXTERNAL_OES,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            EXTERNAL_OES,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(EXTERNAL_OES, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(EXTERNAL_OES, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.bind_texture(EXTERNAL_OES, None);
        Some(texture)
    }
}

/// Wrapper around an `EGLImage` targeting an external GL texture.
pub struct ExternalTexture {
    #[cfg(target_os = "android")]
    display: *mut std::ffi::c_void,
    #[cfg(target_os = "android")]
    image: *mut std::ffi::c_void,
    texture: glow::Texture,
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    owns_texture: bool,
}

#[cfg(target_os = "android")]
mod android {
    use super::*;
    use glow::HasContext;
    use std::ffi::c_void;

    const EGL_NO_CONTEXT: *mut c_void = std::ptr::null_mut();
    const EGL_NATIVE_BUFFER_ANDROID: u32 = 0x3140;
    const EGL_IMAGE_PRESERVED_KHR: u32 = 0x30D2;
    const EGL_TRUE: u32 = 1;
    const EGL_NONE: u32 = 0x3038;

    type EglGetNativeClientBufferAndroid = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
    type EglCreateImageKhr =
        unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut c_void, *const i32) -> *mut c_void;
    type GlEglImageTargetTexture2DOes = unsafe extern "C" fn(u32, *mut c_void);

    #[link(name = "EGL")]
    extern "C" {
        fn eglGetProcAddress(name: *const u8) -> *const c_void;
        fn eglGetError() -> i32;
    }

    fn proc_address(name: &[u8]) -> *const c_void {
        debug_assert!(name.ends_with(&[0]));
        unsafe { eglGetProcAddress(name.as_ptr()) }
    }

    impl ExternalTexture {
        /// Create an EGLImage for `hardware_buffer` and bind it to `texture`.
        /// The texture is not owned by the returned value.
        pub fn from_hardware_buffer_with_texture(
            gl: &glow::Context,
            display: *mut c_void,
            hardware_buffer: *mut c_void,
            texture: glow::Texture,
        ) -> Result<Self, RenderError> {
            if display.is_null() || hardware_buffer.is_null() {
                return Err(RenderError::InvalidExternalBuffer);
            }

            let get_native_buffer: EglGetNativeClientBufferAndroid =
                unsafe { std::mem::transmute(proc_address(b"eglGetNativeClientBufferANDROID\0")) };
            let create_image: EglCreateImageKhr =
                unsafe { std::mem::transmute(proc_address(b"eglCreateImageKHR\0")) };
            let target_texture: GlEglImageTargetTexture2DOes =
                unsafe { std::mem::transmute(proc_address(b"glEGLImageTargetTexture2DOES\0")) };

            let client_buffer = unsafe { get_native_buffer(hardware_buffer) };
            if client_buffer.is_null() {
                return Err(RenderError::ExternalTexture("native client buffer is null"));
            }

            let attributes = [
                EGL_IMAGE_PRESERVED_KHR as i32,
                EGL_TRUE as i32,
                EGL_NONE as i32,
            ];
            let image = unsafe {
                create_image(
                    display,
                    EGL_NO_CONTEXT,
                    EGL_NATIVE_BUFFER_ANDROID,
                    client_buffer,
                    attributes.as_ptr(),
                )
            };
            if image.is_null() {
                log::error!("eglCreateImageKHR failed: 0x{:x}", unsafe { eglGetError() });
                return Err(RenderError::ExternalTexture("eglCreateImageKHR failed"));
            }

            unsafe {
                gl.bind_texture(EXTERNAL_OES, Some(texture));
                target_texture(EXTERNAL_OES, image);
                gl.bind_texture(EXTERNAL_OES, None);
            }

            Ok(Self {
                display,
                image,
                texture,
                owns_texture: false,
            })
        }

        /// Create a temporary external texture plus EGLImage.
        pub fn from_hardware_buffer(
            gl: &glow::Context,
            display: *mut c_void,
            hardware_buffer: *mut c_void,
        ) -> Result<Self, RenderError> {
            let texture = create_external_texture(gl)
                .ok_or(RenderError::ExternalTexture("glGenTextures failed"))?;
            match Self::from_hardware_buffer_with_texture(gl, display, hardware_buffer, texture) {
                Ok(mut external) => {
                    external.owns_texture = true;
                    Ok(external)
                }
                Err(error) => {
                    unsafe { gl.delete_texture(texture) };
                    Err(error)
                }
            }
        }

        pub fn texture(&self) -> glow::Texture {
            self.texture
        }

        pub fn destroy(&mut self, gl: &glow::Context) {
            if self.owns_texture {
                unsafe { gl.delete_texture(self.texture) };
                self.owns_texture = false;
            }
        }
    }

    impl Drop for ExternalTexture {
        fn drop(&mut self) {
            if !self.image.is_null() {
                type Destroy = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32;
                let destroy: Destroy =
                    unsafe { std::mem::transmute(proc_address(b"eglDestroyImageKHR\0")) };
                unsafe {
                    destroy(self.display, self.image);
                }
                self.image = std::ptr::null_mut();
            }
        }
    }
}

#[cfg(not(target_os = "android"))]
impl ExternalTexture {
    pub fn from_hardware_buffer(
        _gl: &glow::Context,
        _display: *mut std::ffi::c_void,
        _hardware_buffer: *mut std::ffi::c_void,
    ) -> Result<Self, RenderError> {
        Err(RenderError::ExternalTexture(
            "external textures require Android",
        ))
    }

    pub fn texture(&self) -> glow::Texture {
        self.texture
    }

    pub fn destroy(&mut self, _gl: &glow::Context) {}
}
