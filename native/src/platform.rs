//! Platform hooks. The Android implementation owns ALVR, EGL, MediaCodec and
//! the render/input threads. The host implementation is a no-op so the whole
//! workspace can be tested without an Android device.

#[cfg(not(target_os = "android"))]
mod host {
    use crate::DisplayInfo;
    use jni::objects::JObject;
    use jni::JNIEnv;
    use phonealvr_core::{Action, Quat};

    pub fn initialize(_display: DisplayInfo) -> Result<(), String> {
        Ok(())
    }

    pub fn set_android_context(_env: &mut JNIEnv, _context: &JObject) {}

    pub fn handle_actions(actions: &[Action]) {
        for action in actions {
            log::debug!("host platform action: {action:?}");
        }
    }

    pub fn shutdown() {}
    pub fn apply_settings(_json: &str) {}
    pub fn apply_viewer_profile() {}

    pub fn surface_created(_env: &mut JNIEnv, _surface: &JObject) -> bool {
        true
    }

    pub fn surface_changed(_width: i32, _height: i32) {}
    pub fn surface_destroyed() {}
    pub fn render_frame() {}
    pub fn on_orientation(_orientation: Quat) {}
}

#[cfg(not(target_os = "android"))]
pub use host::*;

#[cfg(target_os = "android")]
#[path = "platform_android.rs"]
mod android;

#[cfg(target_os = "android")]
pub use android::*;
