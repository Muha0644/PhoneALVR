//! Platform-independent logic for PhoneALVR.
//!
//! This crate deliberately contains no Android, JNI, GL or ALVR code. Every
//! state machine and numeric routine here is exercised by host unit tests and
//! can be reused from the native event/render threads.

pub mod session;
pub mod settings;
pub mod stats;
pub mod tracking;
pub mod view_params;

pub use session::{Action, Event, Session, SessionState, SessionStatus, StreamInfo};
pub use settings::{Resolution, Settings};
pub use stats::{Stats, StatsTracker};
pub use tracking::{reprojection_offset, HeadTracker, ImuSample, Quat, Vec3};
pub use view_params::{EyeView, Fov, ViewParamDedup, ViewParams};
