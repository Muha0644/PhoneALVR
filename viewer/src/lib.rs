//! Viewer-profile parsing, lens-distortion math and stereo geometry.
//!
//! PhoneALVR owns this code: it does not depend on the abandoned Cardboard SDK.
//! The profile format is intentionally compatible with Cardboard's JSON schema
//! and with the protobuf payload embedded in modern Cardboard QR codes.

pub mod distortion;
pub mod foveated;
pub mod mesh;
pub mod profile;
pub mod projection;

pub use distortion::{EyeMapper, LensDistortion};
pub use foveated::{FoveatedConfig, FoveatedEncoding};
pub use mesh::{build_eye_mesh, Mesh};
pub use profile::{
    EyeFov, ProfileError, ProfileSource, VerticalAlignment, ViewerProfile, MAX_PROFILE_BYTES,
};
pub use projection::{projection_matrix, view_rotation_matrix, Mat4};
