# ADR 0012: Foveated encoding support

- Status: accepted
- Date: 2026-11-05

## Context

The target phone (Xiaomi `23124RA7EO`, 2400×1080) cannot decode the full
side-by-side stream at 60 Hz. At 1088×960 per eye the encoded texture is
2176×960; the phone logs thousands of `Decoder saturation` events, drops
packets, renders corrupted/green frames and has hard-rebooted under load.
ALVR's foveated encoding (FFE) reduces the encoded per-eye resolution by
compressing the edges while keeping a high-resolution centre.

## Decision

Implement the ALVR v20.14.1 FFE layout client-side:

1. Port `foveated_encoding_shader_constants` and the `stream.wgsl` FFE mapping
   into `viewer/src/foveated.rs` (done, with unit tests).
2. Use `FoveatedEncoding::encoded_view_resolution` as the per-eye encoded size
   for the MediaCodec width/height, not the base `StreamingStarted`
   resolution (done in `native/src/platform_android.rs`).
3. Compose the FFE UV mapping after lens distortion and before the side-by-side
   half selection in the renderer shader.
4. Only advertise `foveated_encoding: true` in `ClientCapabilities` after the
   shader hookup is visually verified. Until then the server must not send an
   FFE layout the renderer cannot sample.

## Consequences

- FFE is server-driven; the streamer's centre/edge parameters are passed
  through unchanged.
- The encoded texture is smaller, reducing decoder load and overheating.
- The FFE constants are not per-frame state; upload them with the mesh/stream
  config.
- A user opt-out may be added if a device/driver behaves badly, but the default
  should be enabled once validated.
- FFE is not free: edge regions are compressed, and the client must reproduce
  ALVR's right-eye horizontal mirror and aligned-resolution padding exactly.
