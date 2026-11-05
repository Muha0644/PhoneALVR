# Foveated encoding (FFE) handoff

Status: **math, config, decoder sizing and renderer shader hookup complete;
physical-device validation is the next step.** The client still advertises
`foveated_encoding: false` until that validation passes. Read this together
with `docs/dev/HANDOFF.md`.

## Why this is the next priority

The current device is a Xiaomi `23124RA7EO` (2400×1080 panel). Its H.264
decoder cannot sustain the full side-by-side stream:

- 1088×960 per eye → encoded **2176×960** side-by-side.
- Observed: ~16 FPS, thousands of `Decoder saturation` warnings, 92
  `Network dropped video packet` events in ~17 s, green flashes, and eventually
  a hard reboot.
- 1536×1344 per eye → encoded 3072×1344; this fails to configure entirely.
- Many phone H.264 decoders max out at **1920 px encoded width**. FFE must
  reduce the encoded texture so the decoder can keep up.

## What is already implemented

1. `viewer/src/foveated.rs` is a port of ALVR v20.14.1:
   - `alvr/graphics/src/stream.rs::foveated_encoding_shader_constants`
   - `alvr/graphics/resources/stream.wgsl` (the `if ENABLE_FFE` block)
   - `FoveatedConfig`, `FoveatedEncoding`, `encoded_resolution`,
     `map_eye_uv`.
   - Unit tests cover encoded-size reduction, linear edge-ratio=1,
     full-centre linearity, bounds, right-eye mirroring, monotonicity and
     malformed-config sanitisation.
2. `native/src/platform_android.rs`:
   - `foveated_encoding_for_stream()` extracts
     `stream.settings.video.foveated_encoding` and the
     `enable_foveated_encoding` event flag.
   - `StreamingStarted` computes the FFE layout and stores it in
     `Runtime::set_foveated_encoding`.
   - `STREAM_VIEW_WIDTH/HEIGHT` now hold the **encoded per-eye** resolution
     (from `FoveatedEncoding::encoded_view_resolution`) rather than the base
     per-eye resolution. This is what the MediaCodec width/height options use.
   - The decoder log now prints `foveated=true/false` and the encoded size.
3. `ClientCapabilities.foveated_encoding` is still **false**. Do not flip it
   until the shader hookup is verified on the device, otherwise the server
   will send a foveated texture that the renderer samples as a normal image.
4. `render/src/renderer.rs` applies the FFE remap in both fragment shaders:
   - `u_ffe_enabled` / `u_eye_index` plus the twelve `u_ffe_*` `vec2`
     uniforms, ported from the ALVR `stream.wgsl` block.
   - `to_source()` applies `ffe_map()` **after** the distortion-mesh UV and
     reprojection shift, **before** `u_uv_scale`/`u_uv_offset` select the
     side-by-side half.
   - The right eye is mirrored before and after the mapping, matching
     `FoveatedEncoding::map_eye_uv(1, ...)`.
   - Zero `a_left`/`a_right` coefficients (degenerate `edge_ratio == 1`
     layouts) fall back to the identity edge instead of dividing by zero.
   - `StereoRenderer::set_foveated` stores the constants once per stream or
     profile change; non-finite constants disable the remap.
5. `native/src/platform_android.rs` extracts the negotiated layout in
   `foveated_encoding_for_stream`, logs it, stores it in `Runtime`, and
   uploads `FoveatedShaderParams` when the render thread refreshes the mesh.

## Renderer hookup (implemented)

Files:

- `render/src/renderer.rs` — shader + uniforms.
- `render/src/texture.rs` — external texture (already persistent).
- `native/src/platform_android.rs` — pass the FFE constants and eye index.
- `viewer/src/foveated.rs` — source of truth for constants.

### Mapping order

Our mesh UVs already include the lens-distortion mapping from
`viewer::EyeMapper`. The encoded texture is foveated, so compose:

```
output screen UV
  → lens distortion (existing EyeMapper UV)
  → FFE map (viewer::FoveatedEncoding::map_eye_uv)
  → side-by-side half selection (existing shader u_uv_scale/u_uv_offset)
  → encoded texture
```

The ALVR `stream.wgsl` applies FFE to the eye-image UV before the staging
side-by-side split. We do the same, but after our distortion mesh UV.

### Shader uniforms/constants

Add to the fragment shader:

```glsl
uniform int u_ffe_enabled;
uniform int u_eye_index;
uniform vec2 u_ffe_view_ratio;
uniform vec2 u_ffe_edge_ratio;
uniform vec2 u_ffe_c1;
uniform vec2 u_ffe_c2;
uniform vec2 u_ffe_lo_bound;
uniform vec2 u_ffe_hi_bound;
uniform vec2 u_ffe_a_left;
uniform vec2 u_ffe_b_left;
uniform vec2 u_ffe_a_right;
uniform vec2 u_ffe_b_right;
uniform vec2 u_ffe_c_right;
```

Exact WGSL body to port (from ALVR v20.14.1 `stream.wgsl`):

```wgsl
if ENABLE_FFE {
    let view_size_ratio = vec2f(VIEW_WIDTH_RATIO, VIEW_HEIGHT_RATIO);
    let edge_ratio = vec2f(EDGE_X_RATIO, EDGE_Y_RATIO);
    let c1 = vec2f(C1_X, C1_Y);
    let c2 = vec2f(C2_X, C2_Y);
    let lo_bound = vec2f(LO_BOUND_X, LO_BOUND_Y);
    let hi_bound = vec2f(HI_BOUND_X, HI_BOUND_Y);
    let a_left = vec2f(A_LEFT_X, A_LEFT_Y);
    let b_left = vec2f(B_LEFT_X, B_LEFT_Y);
    let a_right = vec2f(A_RIGHT_X, A_RIGHT_Y);
    let b_right = vec2f(B_RIGHT_X, B_RIGHT_Y);
    let c_right = vec2f(C_RIGHT_X, C_RIGHT_Y);

    if pc.view_idx == 1 { corrected_uv.x = 1.0 - corrected_uv.x; }

    let center = (corrected_uv - c1) * edge_ratio / c2;
    let left_edge = (-b_left + sqrt(b_left*b_left + 4.0*a_left*corrected_uv)) / (2.0*a_left);
    let right_edge = (-b_right + sqrt(b_right*b_right - 4.0*(c_right - a_right*corrected_uv))) / (2.0*a_right);

    if corrected_uv.x < lo_bound.x { corrected_uv.x = left_edge.x; }
    else if corrected_uv.x > hi_bound.x { corrected_uv.x = right_edge.x; }
    else { corrected_uv.x = center.x; }

    // same for y

    corrected_uv = corrected_uv * view_size_ratio;

    if pc.view_idx == 1 { corrected_uv.x = 1.0 - corrected_uv.x; }
}
```

Remember the **right-eye mirror**: the server's right-eye encoded image is
mirrored horizontally for FFE. `FoveatedEncoding::map_eye_uv(eye=1, ...)`
already implements the flip before and after the mapping.

### Pass FFE to the renderer

`render::FoveatedShaderParams` (all `[f32;2]`, `Copy`) is built with
`From<viewer::FoveatedEncoding>` and passed to
`StereoRenderer::set_foveated(Option<FoveatedShaderParams>)` once per
stream/profile change. `render()` sets `u_eye_index = eye` per eye; the
constants are shared by both eyes and only the mirror differs. The constants
do **not** change per frame.

`native/src/platform_android.rs` reads `runtime.foveated_encoding()` when it
rebuilds the mesh. `StreamingStarted`/`StreamingStopped` mark the mesh dirty so
the render thread refreshes the constants even when the profile is unchanged.

### Flip the capability

After the shader is verified on the device:

- Set `foveated_encoding: true` in `ClientCapabilities` in
  `native/src/platform_android.rs`.
- Consider a user setting (`Settings.foveated_encoding`, default true) so a
  broken device can opt out; the server setting still controls the actual
  parameters.
- Keep advertising 8-bit SDR/no HDR until colour management is implemented.

## Decoder sizing rules

- `FoveatedEncoding::encoded_resolution` is **per eye**.
- The encoded side-by-side texture width is `encoded_resolution[0] * 2`.
- Use that width/height for the MediaCodec `width`/`height` options
  (`STREAM_VIEW_WIDTH/HEIGHT` already contains it).
- Do not use the base `StreamingStarted.view_resolution` for MediaCodec when
  FFE is active; the SPS/encoded texture is the optimized size.
- The renderer still displays the base per-eye resolution; the FFE UV mapping
  expands the smaller encoded image across the eye viewport.

## Verification checklist

Steps 2–6 are still pending: no device was attached during the shader-hookup
change, so `ClientCapabilities.foveated_encoding` must remain `false`.

1. Unit tests: `cargo test --workspace` (viewer FFE math + render shader
   ordering/uniform tests). Done.
2. Build a release-core APK, install, and set ALVR FFE to Enabled with the
   default parameters (center 0.45/0.4, shift 0.4/0.1, edge 4/5).
3. Confirm log:
   ```text
   streaming started: ... per eye ... foveated=true encoded=<w>x<h>
   decoder config: ... view <w>x<h> (encoded <2w>x<h>)
   ```
   The encoded size should be clearly below the base resolution.
4. Check the image: center sharp, edges compressed but geometrically correct;
   no green/black bands. A checkerboard/grid is the best validation.
5. Watch `Decoder saturation` and `Network dropped` counts. They should drop
   substantially versus the 1088×960/no-FFE run.
6. Soak/thermal test; the phone previously hard-rebooted under load.

## Sources

- `../PhoneVR/android/ALVR/alvr/graphics/src/stream.rs`
  - `foveated_encoding_shader_constants()` around line 442.
- `../PhoneVR/android/ALVR/alvr/graphics/resources/stream.wgsl`
  - `fragment_main` / `if ENABLE_FFE` block.
- `../PhoneVR/android/ALVR/alvr/session/src/settings.rs`
  - `FoveatedEncodingConfig` and its defaults.
