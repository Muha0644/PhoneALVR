# ADR 0009: Viewer-profile format

- Status: accepted
- Date: 2026-11-05

## Decision

Accept both:

1. Cardboard's JSON schema (`interLensDistance`, `screenToLensDistance`,
   `trayToLensDistance`, `distortionCoefficients`, `fov` /
   `left_eye_field_of_view_angles`, `verticalAlignment`).
2. The protobuf payload embedded in modern Cardboard QR URLs
   (`...?p=<base64>`), using the field numbers from `cardboard_device.proto`.

Persist a canonical JSON representation. Validate all numeric fields and never
evaluate imported data.

## Consequences

- The parser is pure Rust and unit-tested with real-shaped payloads.
- Profiles are capped at 64 KiB and at 8 distortion coefficients.
- A QR code from an old Cardboard v1 viewer works, as does a generated modern
  viewer profile.
