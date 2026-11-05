# Compatibility

## ALVR

| Item | Value |
| --- | --- |
| Target ALVR release | **v20.14.1** |
| Upstream revision | `a9f6542fa507a841f40ab4f3fcb531427cd02550` |
| Client core crate | `alvr_client_core` from `alvr-org/ALVR` tag `v20.14.1` |
| Protocol | v20 (client core reports the protocol ID) |
| Newer/older servers | Not supported in v1; fail fast with a clear error |

**Test status:** the native core compiles against the pinned revision and the
host policy tests pass. End-to-end streaming against a live v20.14.1 streamer
has **not yet been recorded** in this repository. Do not claim compatibility
for other releases until the manual test plan has been run and this table is
updated.

### Resolution behaviour

ALVR aligns encoded resolutions down to a multiple of 32 pixels. Upstream
v20.14.1's `ClientCapabilities` exposes `default_view_resolution` but not
`max_view_resolution` (the max-bounds change was a later/backported patch in
the reference project). PhoneALVR therefore advertises the real per-eye panel
resolution as the default and **does not apply a local ALVR patch**. Set
**Video → Resolution → Scale 1.0** on the streamer so it uses the advertised
panel size.

The app validates the negotiated resolution and logs an error if the stream is
larger than the panel.

## Android

| Item | Value |
| --- | --- |
| Minimum SDK | 26 (Android 8.0) |
| Target/compile SDK | 35 |
| Primary ABI | `arm64-v8a` |
| Development ABI | `x86_64` |
| Graphics | OpenGL ES 3.0+ |
| Display orientation | sensor-landscape |

## Devices

| Device | Android | Result | Notes |
| --- | --- | --- | --- |
| — | — | Not yet tested | Add rows after the first real-device pass |

## Known limitations (v1)

- 3DoF only. No 6DoF, controllers, hand/eye tracking or microphone passthrough.
- No Vulkan renderer; GLES 3.0 only.
- No compatibility shim for ALVR releases other than v20.14.1.
- Foveated encoding, 10-bit and HDR are not negotiated in v1; the client
  advertises 8-bit SDR and no foveation. Upscaling settings are accepted but
  ignored.
- The render pacing loop is Kotlin-managed (dedicated thread); moving it fully
  into Rust is planned but not required for correctness.
- One `EGLImage` is created per decoded frame; an image pool is an M4
  optimisation.
- Late latching is a clamped 2D yaw/pitch UV shift, not depth-aware timewarp.
- Colour management is 8-bit SDR; limited/full-range and HDR metadata are not
  applied yet.
- Emulator sensor injection is not a correctness test for tracking.
