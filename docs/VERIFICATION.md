# Verification log

This file records repeatable evidence, not claims. Add a dated entry for every
milestone and keep the commands.

## 2026-11-05 — first vertical slice

Environment: Linux (WSL2), Rust 1.88.0, Android NDK r27c, Windows JDK 21/Android
Studio JBR, Gradle 8.10.2, AGP 8.7.3, Kotlin 2.0.21.

### Host Rust

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Result: pass.

```
phonealvr-core    43 passed
phonealvr-native  15 passed
phonealvr-render   3 passed
phonealvr-viewer  40 passed
total            101 passed
```

The suite covers the state machine and 100 start/stop cycles, reconnect and
handshake failures, settings validation/clamping, resolution alignment, view
param deduplication, tracking prediction/recentering/reprojection, statistics,
Cardboard JSON and protobuf QR parsing, distortion math, mesh topology,
projection math, and the ALVR v20.14.1 foveated-encoding constants/UV mapping.

### Android cross-compilation

```bash
export ANDROID_NDK_HOME=/path/to/android-ndk-r27c
cargo ndk -p 26 -t arm64-v8a check -p phonealvr-native
cargo ndk -p 26 -t x86_64 check -p phonealvr-native
```

Result: pass. Confirms the ALVR v20.14.1 client-core API usage, JNI symbols,
EGL/GLES code and MediaCodec decoder split compile for both ABIs.

### Native core artifacts

```bash
ABIS="arm64-v8a x86_64" PROFILE=debug ./scripts/build-core.sh
ABIS="arm64-v8a" PROFILE=release ./scripts/build-core.sh
```

Result: pass.

```
debug arm64-v8a  libphonealvr.so  20.6 MB
debug x86_64     libphonealvr.so  25.3 MB
release arm64    libphonealvr.so   7.3 MB
```

`readelf -sW` confirms all `Java_dev_phonealvr_client_NativeSession_*` exports
and `JNI_OnLoad` are present.

### Android app

```bash
./gradlew :app:spotlessCheck :app:testDebugUnitTest :app:lintDebug \
          :app:assembleDebug :app:assembleRelease
```

Result: pass.

- Kotlin unit tests: pass.
- Android Lint: 0 errors (warnings only; fixed-orientation and release ABI
  warnings are intentional).
- Debug APK: 86 MB (arm64-v8a + x86_64, unstripped debug symbols).
- Release APK: 18 MB (arm64-v8a, unsigned locally).
- APK contents: `lib/arm64-v8a/libphonealvr.so` (and x86_64 in debug).

### Physical-device evidence (2026-11-05, Xiaomi 23124RA7EO)

- Wired mode connects; surface/EGL/renderer start and a first frame is
  presented.
- 1088×960 per eye → encoded 2176×960: ~16 FPS, 4,741 `Decoder saturation`
  and 92 `Network dropped video packet` events in 17 s; green flashes and a
  hard reboot under load.
- 960×864 per eye → encoded 1920×864: green flashes disappeared, but the phone
  was still overloaded and hard-rebooted. Foveated encoding is required.
- Decoder configuration at 2176×960 fails with `ErrorUnknown`; the phone's
  H.264 decoder appears to max out near 1920 px encoded width.

### Not yet verified

- Real ALVR v20.14.1 streamer + physical phone: first frame, stereo geometry,
  distortion, latency, reconnect, 60-minute soak, 100× app cycles.
- Emulator instrumented tests: no AVD was available in this environment.
- Signing/release workflow: secrets not available locally; the release APK is
  unsigned.
- Perfetto/gfxinfo measurements and the <1% dropped-frame target.

The next evidence entry must include a physical device model, Android build,
ALVR version, PC/GPU, network, logcat excerpt and a trace for performance work.

## 2026-11-05 — foveated-encoding renderer hookup

- `render/src/renderer.rs`: FFE uniforms and `ffe_map()` in both fragment
  shaders. Order is distortion-mesh UV → FFE remap → side-by-side half, with
  the right-eye horizontal mirror. Zero `a_left`/`a_right` coefficients fall
  back to the identity edge.
- `native/src/platform_android.rs`: `foveated_encoding_for_stream` extracts the
  negotiated layout; `StreamingStarted` logs
  `foveated=<bool> encoded=<w>x<h>`, stores the encoded size in
  `STREAM_VIEW_*`/`Runtime`, and marks the mesh pass dirty so the render thread
  uploads `FoveatedShaderParams`.
- `ClientCapabilities.foveated_encoding` is still `false`.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
ANDROID_NDK_HOME=... ABIS=arm64-v8a PROFILE=release ./scripts/build-core.sh
cargo ndk -p 26 -t x86_64 check -p phonealvr-native
```

Result: pass. Host suite 105 tests (43 core + 16 native + 6 render + 40
viewer). `libphonealvr.so` (7.2 MB) rebuilt for arm64-v8a and the debug APK
assembled.

Not verified: an ALVR streamer + physical phone with FFE enabled. No device was
attached (`adb devices` empty), so the green-flash/decoder-saturation/packet
drop comparison and the capability flip are still pending.

## 2026-11-05 — first on-device FFE negotiation

With ALVR "Force enable on smartphone clients" set and the new client installed,
the streamer negotiated FFE:

```text
streaming started: 928x832 per eye @ 60 Hz, foveated=true encoded=544x448
```

- Encoded per-eye size is 544×448 (1088×448 side-by-side), clearly below the
  928×832 base (1856×832 side-by-side).
- The renderer failed to initialize at that point:
  `shader compilation failed: ERROR: 0:39: '=' : cannot convert from '2-component vector of bool' to '2-component vector of float'`.
  The FFE edge-mask vector was declared `vec2` while `greaterThanEqual` returns
  `bvec2`; replaced with a float `step()` mask.
- With no render consumer, the decoder logged 1,360 `Video frame queue
  overflow`, 2,099 `Decoder saturation` drops and 105 `Network dropped video
  packet` events in ~8 s. Decoder configs fell from 100 per run (before the
  dedup fix) to 10 across four stream starts.
- Image quality, green flashes and post-fix saturation counts are still
  unverified: the next run with the corrected shader is required.

## 2026-11-05 — working FFE render

- `foveated=true encoded=544x448`; MediaCodec configured 1088x448. Surface
  created, `first decoded frame` logged, video visible and the app stable.
- The corrected FFE shader compiles; no renderer initialization errors.
- Decoder still reports 9,439 `Decoder saturation` drops, 5,968 queue overflows
  and 421 `Network dropped video packet` events in ~38 s at 544x448 per eye.
  Some drops precede the first frame (surface created ~2.6 s after streaming
  started), but the rate remains high; render-loop pacing is the next perf task.
- Head tracking now converts Android sensor space (natural portrait, ENU) to
  OpenVR space with the display rotation, and Settings has a screen-orientation
  lock (Auto/portrait/landscape/reverse). Device confirmation is pending.

## 2026-11-05 — rotation and pacing corrections

- First landscape test reported inverted up/down and left/right with roll
  unchanged. That is the signature of a 180° rotation applied on the device
  (right) side: the display-rotation sign was reversed. The transform now uses
  a positive Z rotation for `Surface.ROTATION_*`; unit tests cover both
  landscape rotations.
- The render loop used a 60 Hz software pacer **and** a vsync-blocking
  `eglSwapBuffers`, waiting for two vsyncs per frame. It now relies solely on
  the swap interval; empty frames are not swapped. A `render: <fps>` line is
  logged every 5 s for the next device run.

## 2026-11-05 — 144 fps server pacing identified

At scale 1.0 the stream is FFE-encoded at 704×544 per eye (1408×544
side-by-side). The renderer presented 59.9 fps, but the Qualcomm codec reported:

```text
Stats over last 3463ms:
Qinput: 565, DQinput: 565 success out of 1542 tries
Render: 496, Drop: 0, DQoutput: 496 success out of 2520 tries
Avg Input Interval: 6ms, Avg Render Interval: 6ms
```

496 frames in 3.46 s is ~143 fps, matching the 144 Hz desktop while ALVR's
`enforce_server_frame_pacing` was disabled. The phone was decoding 143 fps and
presenting 60, producing the saturation drops, queue overflows, and a 30 Mbps
budget spread over 2.4× too many frames (visible pixelation).

`enforce_server_frame_pacing` was enabled on the running streamer through its
`/api/dashboard-request` endpoint and persisted in `session.json`.

## 2026-11-05 — stereo convergence / lens centres

The saved QR profile is 65 mm IPD, 50° half-FOV, 44 mm screen-to-lens. The code
already halves the IPD twice (client eye positions and server eye transforms),
but both eyes were rendered centred on their half-screen. On a ~152 mm-wide
panel a 65 mm lens pair is inset about 5.5 mm per eye, so the virtual image
centres were ~11 mm too far apart, making convergence uncomfortable despite a
clear image.

Fix: per-eye meshes with `EyeMapper::lens_center_uv` set from
`inter_lens_distance / physical_panel_width`, and `source_uv` now maps the lens
centre to the source image centre. `DisplayInfo` carries `xdpi` from
`DisplayMetrics`. An IPD override (mm) is available in Settings; the effective
separation is logged at stream start as `stereo: eye separation ... mm`.

