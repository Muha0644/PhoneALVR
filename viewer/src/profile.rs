//! Cardboard viewer-profile parsing and validation.
//!
//! Two encodings are accepted:
//! 1. Cardboard's JSON schema (`interLensDistance`, `distortionCoefficients`,
//!    `fov`, ...). This is the format recommended by the project brief.
//! 2. The protobuf payload in a Cardboard QR URL (`...?p=<base64>`). Modern
//!    viewer QR codes use this encoding; the schema is copied from
//!    `cardboard_device.proto` in the Cardboard SDK (Apache-2.0).

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Hard upper bound for imported profile data. The renderer never sees more.
pub const MAX_PROFILE_BYTES: usize = 64 * 1024;
const MAX_DISTORTION_COEFFICIENTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlignment {
    Bottom,
    Center,
    Top,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EyeFov {
    /// Positive magnitudes in degrees, matching Cardboard's
    /// `left_eye_field_of_view_angles` (left, right, bottom, top).
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
}

impl EyeFov {
    pub const fn new(left: f32, right: f32, bottom: f32, top: f32) -> Self {
        Self {
            left,
            right,
            bottom,
            top,
        }
    }

    pub fn symmetric(degrees: f32) -> Self {
        let half = degrees * 0.5;
        Self::new(half, half, half, half)
    }

    pub fn from_degrees_array(values: [f32; 4]) -> Self {
        Self::new(values[0], values[1], values[2], values[3])
    }

    /// Signed tangent extents in the ALVR/OpenGL convention:
    /// left and bottom are negative, right and top are positive.
    pub fn tan_extents(self) -> TanFov {
        let to_rad = std::f32::consts::PI / 180.0;
        TanFov {
            left: -(self.left * to_rad).tan(),
            right: (self.right * to_rad).tan(),
            bottom: -(self.bottom * to_rad).tan(),
            top: (self.top * to_rad).tan(),
        }
    }

    pub fn is_valid(self) -> bool {
        let values = [self.left, self.right, self.bottom, self.top];
        values
            .iter()
            .all(|v| v.is_finite() && *v > 0.0 && *v < 120.0)
            && self.left + self.right < 180.0
            && self.bottom + self.top < 180.0
    }
}

/// Tangent-space field of view. This is the natural space for lens distortion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TanFov {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
}

impl TanFov {
    /// `uv` is in [0, 1], with (0,0) at the top-left of the eye viewport.
    pub fn uv_to_tan(self, uv: [f32; 2]) -> [f32; 2] {
        [
            self.left + uv[0] * (self.right - self.left),
            self.top + uv[1] * (self.bottom - self.top),
        ]
    }

    /// Inverse of [`Self::uv_to_tan`].
    pub fn tan_to_uv(self, tan: [f32; 2]) -> [f32; 2] {
        [
            (tan[0] - self.left) / (self.right - self.left),
            (tan[1] - self.top) / (self.bottom - self.top),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSource {
    Builtin,
    Json,
    QrProtobuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewerProfile {
    pub vendor: String,
    pub model: String,
    pub screen_to_lens_distance: f32,
    pub inter_lens_distance: f32,
    pub tray_to_lens_distance: f32,
    pub vertical_alignment: VerticalAlignment,
    pub fov: EyeFov,
    pub distortion_coefficients: Vec<f32>,
    /// Eye x offsets in metres, derived from the IPD.
    pub eye_positions: [[f32; 3]; 2],
    pub source: ProfileSource,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ProfileError {
    #[error("viewer profile is empty")]
    Empty,
    #[error("viewer profile is too large")]
    TooLarge,
    #[error("unsupported viewer profile encoding")]
    UnsupportedEncoding,
    #[error("invalid viewer profile: {0}")]
    Invalid(String),
}

impl ProfileError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

impl Default for ViewerProfile {
    fn default() -> Self {
        Self::cardboard_v1()
    }
}

impl ViewerProfile {
    /// The built-in fallback. Coefficients and dimensions match the classic
    /// Cardboard v1 profile; if a user scans a QR code it always wins.
    pub fn cardboard_v1() -> Self {
        let inter_lens = 0.060;
        Self {
            vendor: "Google".to_owned(),
            model: "Cardboard v1".to_owned(),
            screen_to_lens_distance: 0.042,
            inter_lens_distance: inter_lens,
            tray_to_lens_distance: 0.035,
            vertical_alignment: VerticalAlignment::Bottom,
            fov: EyeFov::symmetric(90.0),
            distortion_coefficients: vec![0.441, 0.156, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            eye_positions: [[-inter_lens * 0.5, 0.0, 0.0], [inter_lens * 0.5, 0.0, 0.0]],
            source: ProfileSource::Builtin,
        }
    }

    /// Parse JSON, a Cardboard QR URL, or a raw base64 protobuf/JSON payload.
    pub fn parse(input: &str) -> Result<Self, ProfileError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ProfileError::Empty);
        }
        if input.len() > MAX_PROFILE_BYTES {
            return Err(ProfileError::TooLarge);
        }

        if input.starts_with('{') {
            let mut profile = parse_json(input)?;
            profile.source = ProfileSource::Json;
            return Ok(profile);
        }

        let payload = extract_qr_payload(input)?;
        let payload = percent_decode(&payload);
        let decoded = decode_base64(&payload)?;

        if decoded.first() == Some(&b'{') {
            let text =
                std::str::from_utf8(&decoded).map_err(|_| ProfileError::UnsupportedEncoding)?;
            let mut profile = parse_json(text)?;
            profile.source = ProfileSource::Json;
            return Ok(profile);
        }

        let mut profile = parse_protobuf(&decoded)?;
        profile.source = ProfileSource::QrProtobuf;
        Ok(profile)
    }

    pub fn to_json(&self) -> String {
        serde_json::json!({
            "version": "1.0.0",
            "vendor": self.vendor,
            "model": self.model,
            "screenToLensDistance": self.screen_to_lens_distance,
            "interLensDistance": self.inter_lens_distance,
            "trayToLensDistance": self.tray_to_lens_distance,
            "verticalAlignment": match self.vertical_alignment {
                VerticalAlignment::Bottom => "bottom",
                VerticalAlignment::Center => "center",
                VerticalAlignment::Top => "top",
            },
            "distortionCoefficients": self.distortion_coefficients,
            "fov": [self.fov.left, self.fov.right, self.fov.bottom, self.fov.top],
            "eyePositions": self.eye_positions,
        })
        .to_string()
    }

    pub fn ipd(&self) -> f32 {
        self.inter_lens_distance
    }

    /// Validate and normalise. Returns a clear error rather than letting a bad
    /// profile reach the distortion shader.
    pub fn validate(mut self) -> Result<Self, ProfileError> {
        if !(0.005..=0.250).contains(&self.screen_to_lens_distance) {
            return Err(ProfileError::invalid(format!(
                "screen-to-lens distance {} m is outside 0.005..0.250 m",
                self.screen_to_lens_distance
            )));
        }
        if !(0.020..=0.150).contains(&self.inter_lens_distance) {
            return Err(ProfileError::invalid(format!(
                "inter-lens distance {} m is outside 0.020..0.150 m",
                self.inter_lens_distance
            )));
        }
        if !(0.0..=0.150).contains(&self.tray_to_lens_distance) {
            return Err(ProfileError::invalid(
                "tray-to-lens distance is outside 0..0.150 m",
            ));
        }
        if !self.fov.is_valid() {
            return Err(ProfileError::invalid(
                "field-of-view angles must be finite, positive and below 120 degrees",
            ));
        }
        if self.distortion_coefficients.is_empty() {
            return Err(ProfileError::invalid("distortion coefficients are missing"));
        }
        if self.distortion_coefficients.len() > MAX_DISTORTION_COEFFICIENTS {
            return Err(ProfileError::invalid(format!(
                "at most {MAX_DISTORTION_COEFFICIENTS} distortion coefficients are supported"
            )));
        }
        if !self
            .distortion_coefficients
            .iter()
            .all(|coefficient| coefficient.is_finite())
        {
            return Err(ProfileError::invalid(
                "distortion coefficients must be finite",
            ));
        }

        self.vendor = truncate(self.vendor, 128);
        self.model = truncate(self.model, 128);
        self.eye_positions = [
            [-self.inter_lens_distance * 0.5, 0.0, 0.0],
            [self.inter_lens_distance * 0.5, 0.0, 0.0],
        ];
        Ok(self)
    }
}

impl fmt::Display for ViewerProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} (IPD {:.1} mm, screen-lens {:.1} mm)",
            self.vendor,
            self.model,
            self.inter_lens_distance * 1000.0,
            self.screen_to_lens_distance * 1000.0
        )
    }
}

fn truncate(mut value: String, max_chars: usize) -> String {
    if let Some((index, _)) = value.char_indices().nth(max_chars) {
        value.truncate(index);
    }
    value
}

fn get_object<'a>(
    root: &'a serde_json::Value,
    keys: &[&str],
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    for key in keys {
        if let Some(object) = root.get(*key).and_then(|value| value.as_object()) {
            return Some(object);
        }
    }
    root.as_object()
}

fn get_f32(object: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<f32> {
    for key in keys {
        if let Some(value) = object.get(*key) {
            if let Some(number) = value.as_f64() {
                if number.is_finite() {
                    return Some(number as f32);
                }
            }
        }
    }
    None
}

fn get_string(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    for key in keys {
        if let Some(value) = object.get(*key).and_then(|value| value.as_str()) {
            return Some(value.to_owned());
        }
    }
    None
}

fn get_f32_vec(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<Vec<f32>> {
    for key in keys {
        if let Some(values) = object.get(*key).and_then(|value| value.as_array()) {
            if values.is_empty() {
                continue;
            }
            let mut result = Vec::with_capacity(values.len());
            for value in values {
                let number = value.as_f64()?;
                if !number.is_finite() {
                    return None;
                }
                result.push(number as f32);
            }
            return Some(result);
        }
    }
    None
}

fn parse_json(input: &str) -> Result<ViewerProfile, ProfileError> {
    let root: serde_json::Value = serde_json::from_str(input)
        .map_err(|error| ProfileError::invalid(format!("JSON: {error}")))?;
    let object = get_object(&root, &["device", "deviceParams", "viewer", "params"])
        .ok_or_else(|| ProfileError::invalid("expected a JSON object"))?;

    let inter_lens = get_f32(
        object,
        &[
            "interLensDistance",
            "inter_lens_distance",
            "baselineLensDistance",
        ],
    )
    .ok_or_else(|| ProfileError::invalid("interLensDistance is missing"))?;
    let screen_to_lens = get_f32(object, &["screenToLensDistance", "screen_to_lens_distance"])
        .ok_or_else(|| ProfileError::invalid("screenToLensDistance is missing"))?;
    let tray_to_lens =
        get_f32(object, &["trayToLensDistance", "tray_to_lens_distance"]).unwrap_or(0.035);

    let mut distortion_coefficients = get_f32_vec(
        object,
        &[
            "distortionCoefficients",
            "distortion_coefficients",
            "distortion",
        ],
    )
    .unwrap_or_else(|| vec![0.441, 0.156, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);

    // Accept the accidentally nested form some generators emit.
    if distortion_coefficients.len() == 1 && distortion_coefficients[0] == 0.0 {
        distortion_coefficients = vec![0.441, 0.156, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    }

    let fov = if let Some(values) = get_f32_vec(
        object,
        &["left_eye_field_of_view_angles", "leftEyeFieldOfViewAngles"],
    ) {
        if values.len() != 4 {
            return Err(ProfileError::invalid(
                "left_eye_field_of_view_angles must contain four values",
            ));
        }
        EyeFov::from_degrees_array([values[0], values[1], values[2], values[3]])
    } else if let Some(values) = get_f32_vec(object, &["fov"]) {
        match values.as_slice() {
            [single] => EyeFov::symmetric(*single),
            [left, right, bottom, top] => {
                EyeFov::from_degrees_array([*left, *right, *bottom, *top])
            }
            _ => return Err(ProfileError::invalid("fov must be one or four numbers")),
        }
    } else if let Some(single) = get_f32(object, &["fov"]) {
        EyeFov::symmetric(single)
    } else {
        EyeFov::symmetric(90.0)
    };

    let vertical_alignment = match object
        .get("verticalAlignment")
        .or_else(|| object.get("vertical_alignment"))
    {
        Some(value) => match value {
            serde_json::Value::String(value) => match value.to_ascii_lowercase().as_str() {
                "bottom" => VerticalAlignment::Bottom,
                "center" | "centre" => VerticalAlignment::Center,
                "top" => VerticalAlignment::Top,
                other => {
                    return Err(ProfileError::invalid(format!(
                        "unknown vertical alignment {other}"
                    )))
                }
            },
            serde_json::Value::Number(value) => match value.as_u64() {
                Some(0) => VerticalAlignment::Bottom,
                Some(1) => VerticalAlignment::Center,
                Some(2) => VerticalAlignment::Top,
                _ => return Err(ProfileError::invalid("unknown vertical alignment number")),
            },
            _ => return Err(ProfileError::invalid("invalid vertical alignment")),
        },
        None => VerticalAlignment::Bottom,
    };

    ViewerProfile {
        vendor: get_string(object, &["vendor"]).unwrap_or_else(|| "Imported".to_owned()),
        model: get_string(object, &["model"]).unwrap_or_else(|| "Viewer".to_owned()),
        screen_to_lens_distance: screen_to_lens,
        inter_lens_distance: inter_lens,
        tray_to_lens_distance: tray_to_lens,
        vertical_alignment,
        fov,
        distortion_coefficients: std::mem::take(&mut distortion_coefficients),
        eye_positions: [[0.0; 3]; 2],
        source: ProfileSource::Json,
    }
    .validate()
}

fn extract_qr_payload(input: &str) -> Result<String, ProfileError> {
    // Cardboard QR codes are URLs like
    //   http://google.com/cardboard/cfg?p=<base64 protobuf>
    // Some scanners hand back only the base64 value.
    if let Some(query_index) = input.find('?') {
        let query = &input[query_index + 1..];
        for pair in query.split('&') {
            if let Some(value) = pair.strip_prefix("p=") {
                return Ok(value.to_owned());
            }
        }
        return Err(ProfileError::invalid("URL does not contain a p= payload"));
    }

    if input.starts_with("http://") || input.starts_with("https://") {
        return Err(ProfileError::invalid("URL does not contain a p= payload"));
    }

    // Reject arbitrary text early; the decoder is still bounded.
    if input.len() > MAX_PROFILE_BYTES {
        return Err(ProfileError::TooLarge);
    }
    Ok(input.to_owned())
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = hex_value(bytes[index + 1]);
            let low = hex_value(bytes[index + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                output.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_base64(input: &str) -> Result<Vec<u8>, ProfileError> {
    let trimmed = input.trim();
    let engines = [
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::STANDARD,
    ];
    for engine in engines {
        if let Ok(decoded) = engine.decode(trimmed) {
            if !decoded.is_empty() {
                return Ok(decoded);
            }
        }
    }
    Err(ProfileError::UnsupportedEncoding)
}

fn read_varint(bytes: &[u8], index: &mut usize) -> Result<u64, ProfileError> {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        if *index >= bytes.len() || shift >= 64 {
            return Err(ProfileError::invalid("truncated protobuf varint"));
        }
        let byte = bytes[*index];
        *index += 1;
        result |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
    }
}

fn read_f32(bytes: &[u8], index: &mut usize) -> Result<f32, ProfileError> {
    if *index + 4 > bytes.len() {
        return Err(ProfileError::invalid("truncated protobuf float"));
    }
    let value = f32::from_le_bytes([
        bytes[*index],
        bytes[*index + 1],
        bytes[*index + 2],
        bytes[*index + 3],
    ]);
    *index += 4;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ProfileError::invalid("non-finite protobuf float"))
    }
}

fn skip_field(bytes: &[u8], index: &mut usize, wire_type: u64) -> Result<(), ProfileError> {
    match wire_type {
        0 => {
            read_varint(bytes, index)?;
        }
        1 => {
            if *index + 8 > bytes.len() {
                return Err(ProfileError::invalid("truncated protobuf fixed64"));
            }
            *index += 8;
        }
        2 => {
            let length = read_varint(bytes, index)? as usize;
            if *index + length > bytes.len() {
                return Err(ProfileError::invalid("truncated protobuf bytes"));
            }
            *index += length;
        }
        5 => {
            if *index + 4 > bytes.len() {
                return Err(ProfileError::invalid("truncated protobuf fixed32"));
            }
            *index += 4;
        }
        _ => return Err(ProfileError::invalid("unsupported protobuf wire type")),
    }
    Ok(())
}

fn parse_packed_f32(bytes: &[u8], index: &mut usize) -> Result<Vec<f32>, ProfileError> {
    let length = read_varint(bytes, index)? as usize;
    if *index + length > bytes.len() || length % 4 != 0 {
        return Err(ProfileError::invalid("malformed packed protobuf floats"));
    }
    let end = *index + length;
    let mut values = Vec::with_capacity(length / 4);
    while *index < end {
        values.push(read_f32(bytes, index)?);
    }
    Ok(values)
}

fn parse_protobuf(bytes: &[u8]) -> Result<ViewerProfile, ProfileError> {
    let mut vendor = String::new();
    let mut model = String::new();
    let mut screen_to_lens = None;
    let mut inter_lens = None;
    let mut tray_to_lens = 0.035;
    let mut fov_angles: Vec<f32> = Vec::new();
    let mut coefficients: Vec<f32> = Vec::new();
    let mut vertical_alignment = VerticalAlignment::Bottom;

    let mut index = 0usize;
    while index < bytes.len() {
        let key = read_varint(bytes, &mut index)?;
        let field = key >> 3;
        let wire_type = key & 0x7;
        match field {
            1 | 2 => {
                if wire_type != 2 {
                    return Err(ProfileError::invalid("expected protobuf string"));
                }
                let length = read_varint(bytes, &mut index)? as usize;
                if index + length > bytes.len() {
                    return Err(ProfileError::invalid("truncated protobuf string"));
                }
                let text = String::from_utf8_lossy(&bytes[index..index + length]).into_owned();
                index += length;
                if field == 1 {
                    vendor = text;
                } else {
                    model = text;
                }
            }
            3 | 4 | 6 => {
                let value = match wire_type {
                    5 => read_f32(bytes, &mut index)?,
                    2 => {
                        let values = parse_packed_f32(bytes, &mut index)?;
                        *values
                            .first()
                            .ok_or_else(|| ProfileError::invalid("empty protobuf float"))?
                    }
                    _ => return Err(ProfileError::invalid("expected protobuf float")),
                };
                match field {
                    3 => screen_to_lens = Some(value),
                    4 => inter_lens = Some(value),
                    6 => tray_to_lens = value,
                    _ => {}
                }
            }
            5 => match wire_type {
                2 => fov_angles = parse_packed_f32(bytes, &mut index)?,
                5 => fov_angles.push(read_f32(bytes, &mut index)?),
                _ => return Err(ProfileError::invalid("expected protobuf FOV floats")),
            },
            7 => match wire_type {
                2 => coefficients = parse_packed_f32(bytes, &mut index)?,
                5 => coefficients.push(read_f32(bytes, &mut index)?),
                _ => return Err(ProfileError::invalid("expected protobuf distortion floats")),
            },
            11 => {
                if wire_type != 0 {
                    return Err(ProfileError::invalid("expected protobuf enum"));
                }
                vertical_alignment = match read_varint(bytes, &mut index)? {
                    0 => VerticalAlignment::Bottom,
                    1 => VerticalAlignment::Center,
                    2 => VerticalAlignment::Top,
                    _ => return Err(ProfileError::invalid("unknown vertical alignment")),
                };
            }
            _ => skip_field(bytes, &mut index, wire_type)?,
        }
    }

    let fov = if fov_angles.len() == 4 {
        EyeFov::from_degrees_array([fov_angles[0], fov_angles[1], fov_angles[2], fov_angles[3]])
    } else if fov_angles.is_empty() {
        EyeFov::symmetric(90.0)
    } else {
        return Err(ProfileError::invalid(
            "protobuf FOV must contain four angles",
        ));
    };

    if coefficients.is_empty() {
        coefficients = vec![0.441, 0.156, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    }

    ViewerProfile {
        vendor: if vendor.is_empty() {
            "Cardboard".to_owned()
        } else {
            vendor
        },
        model: if model.is_empty() {
            "Imported viewer".to_owned()
        } else {
            model
        },
        screen_to_lens_distance: screen_to_lens
            .ok_or_else(|| ProfileError::invalid("screen_to_lens_distance is missing"))?,
        inter_lens_distance: inter_lens
            .ok_or_else(|| ProfileError::invalid("inter_lens_distance is missing"))?,
        tray_to_lens_distance: tray_to_lens,
        vertical_alignment,
        fov,
        distortion_coefficients: coefficients,
        eye_positions: [[0.0; 3]; 2],
        source: ProfileSource::QrProtobuf,
    }
    .validate()
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1_JSON: &str = r#"{
        "version": "1.0.0",
        "vendor": "Google",
        "model": "Cardboard v1",
        "interLensDistance": 0.06,
        "baselineLensDistance": 0.06,
        "screenToLensDistance": 0.042,
        "trayToLensDistance": 0.035,
        "distortionCoefficients": [0.441, 0.156, 0, 0, 0, 0, 0, 0],
        "fov": 90,
        "eyePositions": [[0.03, 0.042, 0.035], [-0.03, 0.042, 0.035]]
    }"#;

    #[test]
    fn builtin_is_valid_and_finite() {
        let profile = ViewerProfile::cardboard_v1();
        assert!(profile.clone().validate().is_ok());
        assert_eq!(profile.ipd(), 0.060);
        assert_eq!(profile.fov.left, 45.0);
    }

    #[test]
    fn parses_classic_cardboard_json() {
        let profile = ViewerProfile::parse(V1_JSON).unwrap();
        assert_eq!(profile.vendor, "Google");
        assert_eq!(profile.model, "Cardboard v1");
        assert!((profile.inter_lens_distance - 0.06).abs() < 1e-6);
        assert!((profile.screen_to_lens_distance - 0.042).abs() < 1e-6);
        assert_eq!(profile.distortion_coefficients[0], 0.441);
        assert_eq!(profile.fov.top, 45.0);
        assert_eq!(profile.source, ProfileSource::Json);
    }

    #[test]
    fn parses_four_value_fov_and_snake_case() {
        let json = r#"{
            "vendor": "Acme",
            "model": "Wide",
            "inter_lens_distance": 0.064,
            "screen_to_lens_distance": 0.039,
            "tray_to_lens_distance": 0.035,
            "distortion_coefficients": [0.34, 0.55],
            "left_eye_field_of_view_angles": [50, 50, 40, 40]
        }"#;
        let profile = ViewerProfile::parse(json).unwrap();
        assert_eq!(profile.fov.left, 50.0);
        assert_eq!(profile.fov.bottom, 40.0);
        assert_eq!(profile.distortion_coefficients.len(), 2);
    }

    #[test]
    fn parses_nested_device_object() {
        let json = r#"{
            "formatVersion": 2,
            "device": {
                "vendor": "Nested",
                "interLensDistance": 0.064,
                "screenToLensDistance": 0.039,
                "distortionCoefficients": [0.34, 0.55],
                "fov": 80
            }
        }"#;
        let profile = ViewerProfile::parse(json).unwrap();
        assert_eq!(profile.vendor, "Nested");
        assert_eq!(profile.fov.left, 40.0);
    }

    #[test]
    fn rejects_out_of_range_profiles() {
        let json = r#"{
            "interLensDistance": 999,
            "screenToLensDistance": 0.042
        }"#;
        assert!(matches!(
            ViewerProfile::parse(json),
            Err(ProfileError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_missing_required_fields() {
        assert!(ViewerProfile::parse(r#"{"vendor":"x"}"#).is_err());
        assert!(ViewerProfile::parse("").is_err());
    }

    #[test]
    fn rejects_oversized_input() {
        let input = "x".repeat(MAX_PROFILE_BYTES + 1);
        assert_eq!(ViewerProfile::parse(&input), Err(ProfileError::TooLarge));
    }

    fn write_varint(mut value: u64, out: &mut Vec<u8>) {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if value == 0 {
                break;
            }
        }
    }

    fn write_key(field: u64, wire: u64, out: &mut Vec<u8>) {
        write_varint((field << 3) | wire, out);
    }

    fn write_string(field: u64, value: &str, out: &mut Vec<u8>) {
        write_key(field, 2, out);
        write_varint(value.len() as u64, out);
        out.extend_from_slice(value.as_bytes());
    }

    fn write_float(field: u64, value: f32, out: &mut Vec<u8>) {
        write_key(field, 5, out);
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn write_packed_floats(field: u64, values: &[f32], out: &mut Vec<u8>) {
        write_key(field, 2, out);
        write_varint((values.len() * 4) as u64, out);
        for value in values {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn protobuf_profile() -> Vec<u8> {
        let mut bytes = Vec::new();
        write_string(1, "Google", &mut bytes);
        write_string(2, "Cardboard v2", &mut bytes);
        write_float(3, 0.039, &mut bytes);
        write_float(4, 0.064, &mut bytes);
        write_packed_floats(5, &[50.0, 50.0, 40.0, 40.0], &mut bytes);
        write_float(6, 0.035, &mut bytes);
        write_packed_floats(7, &[0.34, 0.55, 0.0, 0.0], &mut bytes);
        write_key(11, 0, &mut bytes);
        write_varint(1, &mut bytes); // CENTER
        bytes
    }

    #[test]
    fn parses_protobuf_qr_payload() {
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(protobuf_profile());
        let url = format!("http://google.com/cardboard/cfg?p={encoded}");
        let profile = ViewerProfile::parse(&url).unwrap();
        assert_eq!(profile.vendor, "Google");
        assert_eq!(profile.model, "Cardboard v2");
        assert_eq!(profile.fov.left, 50.0);
        assert_eq!(profile.vertical_alignment, VerticalAlignment::Center);
        assert_eq!(profile.source, ProfileSource::QrProtobuf);
    }

    #[test]
    fn parses_raw_base64_payload() {
        let encoded = base64::engine::general_purpose::STANDARD.encode(protobuf_profile());
        let profile = ViewerProfile::parse(&encoded).unwrap();
        assert_eq!(profile.model, "Cardboard v2");
    }

    #[test]
    fn parses_percent_encoded_qr_payload() {
        let encoded = base64::engine::general_purpose::STANDARD.encode(protobuf_profile());
        let encoded = encoded.replace('+', "%2B").replace('/', "%2F");
        let url = format!("https://example.invalid/viewer?p={encoded}");
        let profile = ViewerProfile::parse(&url).unwrap();
        assert_eq!(profile.model, "Cardboard v2");
    }

    #[test]
    fn rejects_garbage_qr_payload() {
        assert!(ViewerProfile::parse("http://example.invalid/cfg").is_err());
        assert!(ViewerProfile::parse("not base64 !!!").is_err());
    }

    #[test]
    fn json_round_trip_is_stable() {
        let profile = ViewerProfile::parse(V1_JSON).unwrap();
        let round_trip = ViewerProfile::parse(&profile.to_json()).unwrap();
        assert!((round_trip.inter_lens_distance - profile.inter_lens_distance).abs() < 1e-6);
        assert_eq!(
            round_trip.distortion_coefficients,
            profile.distortion_coefficients
        );
    }

    #[test]
    fn tan_extents_match_known_angles() {
        let fov = EyeFov::symmetric(90.0);
        let tan = fov.tan_extents();
        assert!((tan.left + 1.0).abs() < 1e-5);
        assert!((tan.right - 1.0).abs() < 1e-5);
        assert!((tan.bottom + 1.0).abs() < 1e-5);
        assert!((tan.top - 1.0).abs() < 1e-5);
    }
}
