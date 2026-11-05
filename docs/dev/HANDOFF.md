# Repository handoff — current state and next steps

Read this first when starting a fresh context.

## Project state

PhoneALVR is a new Android/ALVR client. The repository has working wired mode,
a renderer, viewer/distortion math, and a host test suite.

- Device under test: Xiaomi `23124RA7EO` (sapphiren), 2400×1080 panel.
- ALVR streamer: **v20.14.1** (pinned client core).
- App ID: **`dev.phonealvr.client`** for debug and release (required for wired
  mode).
- Wired mode: works; the app auto-starts listening when resumed.
- Renderer: GLES 3.0, external `AHardwareBuffer` → `EGLImage` texture, stereo
  side-by-side, lens distortion mesh, brightness/vignette/CA, 2D late-latching.
- Current blocker: decoder overload at high encoded resolution. The phone can
  see SteamVR, but the stream is slow/glitchy/green and has hard-rebooted.
- Foveated encoding is **obligatory**. The FFE math, config, decoder sizing
  and renderer shader hookup are done; physical-device validation is next.
  `ClientCapabilities.foveated_encoding` is still `false` until then. See
  `docs/dev/FFE_HANDOFF.md`.

## Last known device evidence

From `Desktop/phonealvr-wired.txt`:

- 1088×960 per eye → encoded 2176×960 side-by-side.
- ~16 FPS, 4,741 `Decoder saturation`, 92 `Network dropped video packet` in
  17 s.
- 337 decoder configure failures at 2176×960 in the earlier process.
- `first decoded frame` present; renderer/EGL/surface all succeeded.
- Green flashes correlate with decoder saturation/packet loss, not a shader
  crash.
- After lowering to ~960×864 (encoded 1920×864) the user saw no green flashes
  but the phone was still overloaded and hard-rebooted. FFE is required to
  reduce the encoded texture further.

## Current build commands

```bash
cd /mnt/c/Users/muha0644.LAPTOP-QV08RKD9/Documents/code/PhoneALVR
export ANDROID_NDK_HOME="$HOME/Android/android-ndk-r27c"

# host checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# native core (debug for logs, release for performance)
ABIS="arm64-v8a" PROFILE=debug   ./scripts/build-core.sh
ABIS="arm64-v8a" PROFILE=release ./scripts/build-core.sh

# app
cmd.exe /c 'cd /d C:\Users\muha0644.LAPTOP-QV08RKD9\Documents\code\PhoneALVR && set "JAVA_HOME=C:\Program Files\Android\Android Studio\jbr" && gradlew.bat :app:assembleDebug'
```

`local.properties` must exist (git-ignored) for Gradle on the Windows SDK.

## Test/verification state

- Rust host: 43 core + 16 native + 6 render + 40 viewer = **105 tests**.
- Android cross-check arm64/x86_64: passing.
- Kotlin unit tests, Android Lint, Spotless, debug APK: passing.
- No emulator/AVD available in the last environment.
- Physical-device FFE validation is pending (no device attached after the
  shader hookup).

## Next tasks (priority order)

1. Device test with FFE at 60 Hz, 30–50 Mbps, release core; record FPS,
   `Decoder saturation`, dropped packets and temperature. Confirm
   `foveated=true` and an encoded size below the base resolution in the log.
2. Enable/advertise `foveated_encoding: true` only after step 1 passes.
3. If FFE works, tune the distortion/FFE composition and add a checkerboard
   validation scene.
5. Remaining reliability/perf work:
   - EGLImage pool (currently one image create/destroy per frame).
   - Remove the short stats lock from the render thread.
   - Colour/range/gamma handling.
   - 60-minute soak / 100× lifecycle / reconnect tests.
   - Emulator instrumented tests + CI emulator job.
   - Signed release workflow.

## Things to be careful about

- Do **not** enable FFE in `ClientCapabilities` before the shader mapping is
  verified on the device; the server will send a foveated layout that the
  current shader samples as a normal image.
- The FFE mapping must be composed **after** lens distortion and **before** the
  side-by-side half selection.
- Right-eye FFE is horizontally mirrored by ALVR.
- MediaCodec width/height must use the **encoded** per-eye size
  (`FoveatedEncoding::encoded_view_resolution`), not the base
  `StreamingStarted.view_resolution`.
- `ClientCapabilities` in ALVR v20.14.1 has no `max_view_resolution`; do not
  re-add it without a supported streamer/backport.
- Keep the app in the foreground for wired mode (`is_activity_resumed` check).
- The phone has hard-rebooted under thermal/decoder load; use the release core
  and lower settings for tests.
- No secrets/keystores in the repo.

## Key files

```
viewer/src/foveated.rs       FFE math + tests (new)
native/src/runtime.rs        Runtime; stores FFE encoding
native/src/platform_android.rs  ALVR events, decoder config, FFE extraction
render/src/renderer.rs       GLES stereo shader/uniforms
render/src/texture.rs        external AHardwareBuffer texture
app/src/main/java/.../ui/HomeScreen.kt  UI/icons/warnings
docs/adr/0012-foveated-encoding.md
docs/ARCHITECTURE.md
docs/COMPATIBILITY.md
```
