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
