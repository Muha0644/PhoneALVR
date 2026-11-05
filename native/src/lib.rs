//! Native boundary for PhoneALVR.
//!
//! Keep this crate thin. Session policy lives in `runtime`, platform code in
//! `platform`, and JNI is only a translation layer. No protocol or rendering
//! logic belongs in a JNI function.

mod jni_api;
mod platform;
mod runtime;

use std::sync::{Mutex, MutexGuard, OnceLock};

pub use runtime::{view_params_for, DisplayInfo, Runtime};

/// Global controller. Android has exactly one VR session per process; the
/// Kotlin side is responsible for not creating two activities at once.
static RUNTIME: OnceLock<Mutex<Option<Runtime>>> = OnceLock::new();

pub(crate) fn runtime_slot() -> &'static Mutex<Option<Runtime>> {
    RUNTIME.get_or_init(|| Mutex::new(None))
}

pub(crate) fn lock_runtime() -> MutexGuard<'static, Option<Runtime>> {
    runtime_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_slot_starts_empty() {
        // This test only runs once; the global is intentionally process-wide.
        let slot = runtime_slot();
        assert!(slot.lock().unwrap().is_none());
    }
}
