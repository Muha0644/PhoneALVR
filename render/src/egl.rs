//! Minimal Android EGL 1.4 + GLES 3 context management.
//!
//! We deliberately use the platform EGL ABI instead of a wrapper crate: the
//! surface is an `ANativeWindow` obtained from the Java `Surface`, and the
//! context must be created on the render thread that owns it.

use std::ffi::c_void;
use std::ptr;

type EglDisplay = *mut c_void;
type EglConfig = *mut c_void;
type EglContextHandle = *mut c_void;
type EglSurface = *mut c_void;
type EglInt = i32;

const EGL_NO_DISPLAY: EglDisplay = ptr::null_mut();
const EGL_NO_CONTEXT: EglContextHandle = ptr::null_mut();
const EGL_NO_SURFACE: EglSurface = ptr::null_mut();
const EGL_DEFAULT_DISPLAY: *mut c_void = ptr::null_mut();
const EGL_FALSE: u32 = 0;
const EGL_TRUE: u32 = 1;

const EGL_SURFACE_TYPE: EglInt = 0x3033;
const EGL_WINDOW_BIT: EglInt = 0x0004;
const EGL_RENDERABLE_TYPE: EglInt = 0x3040;
const EGL_OPENGL_ES3_BIT: EglInt = 0x0040;
const EGL_RED_SIZE: EglInt = 0x3024;
const EGL_GREEN_SIZE: EglInt = 0x3023;
const EGL_BLUE_SIZE: EglInt = 0x3022;
const EGL_ALPHA_SIZE: EglInt = 0x3021;
const EGL_DEPTH_SIZE: EglInt = 0x3025;
const EGL_STENCIL_SIZE: EglInt = 0x3026;
const EGL_NONE: EglInt = 0x3038;
const EGL_CONTEXT_CLIENT_VERSION: EglInt = 0x3098;

#[link(name = "EGL")]
extern "C" {
    fn eglGetDisplay(native_display: *mut c_void) -> EglDisplay;
    fn eglInitialize(display: EglDisplay, major: *mut EglInt, minor: *mut EglInt) -> u32;
    fn eglChooseConfig(
        display: EglDisplay,
        attribs: *const EglInt,
        configs: *mut EglConfig,
        config_size: EglInt,
        count: *mut EglInt,
    ) -> u32;
    fn eglCreateContext(
        display: EglDisplay,
        config: EglConfig,
        share_context: EglContextHandle,
        attribs: *const EglInt,
    ) -> EglContextHandle;
    fn eglCreateWindowSurface(
        display: EglDisplay,
        config: EglConfig,
        window: *mut c_void,
        attribs: *const EglInt,
    ) -> EglSurface;
    fn eglMakeCurrent(
        display: EglDisplay,
        draw: EglSurface,
        read: EglSurface,
        context: EglContextHandle,
    ) -> u32;
    fn eglSwapBuffers(display: EglDisplay, surface: EglSurface) -> u32;
    fn eglSwapInterval(display: EglDisplay, interval: EglInt) -> u32;
    fn eglDestroySurface(display: EglDisplay, surface: EglSurface) -> u32;
    fn eglDestroyContext(display: EglDisplay, context: EglContextHandle) -> u32;
    fn eglTerminate(display: EglDisplay) -> u32;
    fn eglGetProcAddress(name: *const u8) -> *const c_void;
    fn eglGetError() -> EglInt;
}

#[derive(Debug, thiserror::Error)]
pub enum EglError {
    #[error("EGL is unavailable (0x{0:x})")]
    Unavailable(u32),
    #[error("no EGL config matches the GLES3 window requirements (0x{0:x})")]
    NoConfig(u32),
    #[error("eglCreateContext failed (0x{0:x})")]
    Context(u32),
    #[error("eglCreateWindowSurface failed (0x{0:x})")]
    Surface(u32),
    #[error("eglMakeCurrent failed (0x{0:x})")]
    MakeCurrent(u32),
}

pub struct EglContext {
    display: EglDisplay,
    config: EglConfig,
    context: EglContextHandle,
    surface: EglSurface,
    initialized: bool,
}

impl EglContext {
    /// Create a GLES 3 context and window surface for an `ANativeWindow`.
    ///
    /// # Safety
    /// `window` must be a valid `ANativeWindow*` that outlives this context.
    pub unsafe fn create(window: *mut c_void) -> Result<Self, EglError> {
        if window.is_null() {
            return Err(EglError::Surface(0));
        }

        let display = eglGetDisplay(EGL_DEFAULT_DISPLAY);
        if display == EGL_NO_DISPLAY {
            return Err(EglError::Unavailable(eglGetError() as u32));
        }

        let mut major = 0;
        let mut minor = 0;
        if eglInitialize(display, &mut major, &mut minor) != EGL_TRUE {
            return Err(EglError::Unavailable(eglGetError() as u32));
        }

        let config_attribs = [
            EGL_SURFACE_TYPE,
            EGL_WINDOW_BIT,
            EGL_RENDERABLE_TYPE,
            EGL_OPENGL_ES3_BIT,
            EGL_RED_SIZE,
            8,
            EGL_GREEN_SIZE,
            8,
            EGL_BLUE_SIZE,
            8,
            EGL_ALPHA_SIZE,
            8,
            EGL_DEPTH_SIZE,
            0,
            EGL_STENCIL_SIZE,
            0,
            EGL_NONE,
        ];
        let mut config: EglConfig = ptr::null_mut();
        let mut count = 0;
        if eglChooseConfig(display, config_attribs.as_ptr(), &mut config, 1, &mut count)
            == EGL_FALSE
            || count == 0
        {
            eglTerminate(display);
            return Err(EglError::NoConfig(eglGetError() as u32));
        }

        let context_attribs = [EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE];
        let context = eglCreateContext(display, config, EGL_NO_CONTEXT, context_attribs.as_ptr());
        if context == EGL_NO_CONTEXT {
            eglTerminate(display);
            return Err(EglError::Context(eglGetError() as u32));
        }

        let surface = eglCreateWindowSurface(display, config, window, ptr::null());
        if surface == EGL_NO_SURFACE {
            eglDestroyContext(display, context);
            eglTerminate(display);
            return Err(EglError::Surface(eglGetError() as u32));
        }

        let egl = Self {
            display,
            config,
            context,
            surface,
            initialized: true,
        };
        egl.make_current()?;
        Ok(egl)
    }

    pub fn make_current(&self) -> Result<(), EglError> {
        let result =
            unsafe { eglMakeCurrent(self.display, self.surface, self.surface, self.context) };
        if result != EGL_TRUE {
            return Err(EglError::MakeCurrent(unsafe { eglGetError() } as u32));
        }
        Ok(())
    }

    pub fn swap_buffers(&self) {
        unsafe {
            eglSwapBuffers(self.display, self.surface);
        }
    }

    /// Ask `eglSwapBuffers` to block until the next vsync. The render loop is
    /// driven by this instead of a second software timer, which would otherwise
    /// halve the frame rate by waiting for two vsyncs per frame.
    pub fn set_swap_interval(&self, interval: i32) {
        unsafe {
            eglSwapInterval(self.display, interval);
        }
    }

    /// `eglPresentationTimeANDROID` lets the compositor align submission with
    /// the panel's vsync. It is optional; silently skip when unsupported.
    pub fn set_presentation_time(&self, target_ns: u64) {
        type PresentationTime = unsafe extern "C" fn(EglDisplay, EglSurface, u64) -> u32;
        unsafe {
            let address = eglGetProcAddress(b"eglPresentationTimeANDROID\0".as_ptr());
            if !address.is_null() {
                let function: PresentationTime = std::mem::transmute(address);
                function(self.display, self.surface, target_ns);
            }
        }
    }

    pub fn display(&self) -> *mut c_void {
        self.display
    }

    pub fn context(&self) -> *mut c_void {
        self.context
    }

    pub fn config(&self) -> *mut c_void {
        self.config
    }

    /// Loader suitable for `glow::Context::from_loader_function`.
    pub unsafe fn glow_loader(name: &str) -> *const c_void {
        let mut bytes = Vec::with_capacity(name.len() + 1);
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0);
        eglGetProcAddress(bytes.as_ptr())
    }
}

impl Drop for EglContext {
    fn drop(&mut self) {
        unsafe {
            eglMakeCurrent(self.display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
            if self.surface != EGL_NO_SURFACE {
                eglDestroySurface(self.display, self.surface);
                self.surface = EGL_NO_SURFACE;
            }
            if self.context != EGL_NO_CONTEXT {
                eglDestroyContext(self.display, self.context);
                self.context = EGL_NO_CONTEXT;
            }
            if self.initialized {
                eglTerminate(self.display);
                self.initialized = false;
            }
        }
    }
}
