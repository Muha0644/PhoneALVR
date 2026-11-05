# PhoneALVR repository guide (for coding agents)

## What this is

PhoneALVR is a new Android VR client for a stock **ALVR v20.14.1** streamer.
ALVR's PC streamer is the TCP client; the phone advertises and listens. The
phone never dials the PC.

Do **not** copy the architecture of `../PhoneVR` (C++ glue, Cardboard AAR,
GLSurfaceView Java GL loop, committed ALVR submodule). Reuse its protocol
knowledge only. `PROMPT.md` is the source of truth for requirements; the ADRs
in `docs/adr/` record the OPEN decisions.

## Layout

```
app/      Kotlin/Compose UI, service, sensor, Surface, JNI wrapper
native/   Rust cdylib: JNI + Runtime + Android ALVR/EGL/MediaCodec backend
core/     Pure Rust policy: session, settings, tracking, stats, view dedup
viewer/   Pure Rust: Cardboard JSON/protobuf, distortion, mesh, projection
render/   Rust GLES renderer, EGL context, external textures
scripts/  bootstrap.sh, build-core.sh, test.sh
docs/     architecture, compatibility, troubleshooting, manual tests, ADRs
```

## Commands

```bash
./scripts/bootstrap.sh
make core                 # native core for arm64-v8a
ABIS="arm64-v8a x86_64" PROFILE=debug ./scripts/build-core.sh
make build                # debug APK (uses prebuilt jniLibs)
make test                 # cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
./gradlew :app:testDebugUnitTest :app:lintDebug
```

The normal Gradle build never invokes Cargo. If `app/src/main/jniLibs` is
missing, run `make core` first.

## Pins

- ALVR `v20.14.1` (`Cargo.toml`, `Cargo.lock`, `Cargo.toml` workspace deps)
- Rust `1.88.0` (`rust-toolchain.toml`)
- Gradle 8.10.2, AGP 8.7.3, Kotlin 2.0.21 (`gradle/`)
- NDK r27c, cargo-ndk 3.5.4 (`scripts/`)
- App ID `dev.phonealvr.client`, debug suffix `.debug`

Bump pins in the same change as `docs/COMPATIBILITY.md`, the ADR and the
workflows. Do not introduce a local ALVR patch file.

## Architecture rules

- `core`, `viewer`, `render` must stay Android-independent and host-testable.
- `native::Runtime` owns policy. JNI functions only translate and delegate.
- No protocol/business logic in JNI, `onDrawFrame`, or Compose.
- The event thread drains ALVR events; the render thread never polls ALVR.
- View parameters are sent only when they change. Never on a tracking timer.
- Teardown must be idempotent from every state. `Session::handle(Close)` is the
  model.
- No per-frame heap allocation, lock or blocking I/O on the render thread in
  steady state. Profile/mesh changes are applied at frame start.
- Zero-copy decode: decoder `AHardwareBuffer` → `EGLImage` → external texture.
- The phone listens; never add code that dials the streamer.

## JNI surface

Keep it small; update both sides together:

`nativeCreate`, `nativeDestroy`, `nativeStart`, `nativeStop`,
`nativeUpdateSettings`, `nativeSetViewerProfile`, `nativeOnSurfaceCreated`,
`nativeOnSurfaceChanged`, `nativeOnSurfaceDestroyed`, `nativeRenderFrame`,
`nativeRecenter`, `nativePushOrientation`, `nativePollStatus`, `nativeGetStats`,
`nativeGetDiagnostics`.

JNI names are `Java_dev_phonealvr_client_NativeSession_*`; Kotlin uses a
companion object with `@JvmStatic`.

## Tests first/with the change

- State machine, settings, tracking, stats, dedup: `core/src/*` unit tests.
- Viewer parsing/distortion/mesh/projection: `viewer/src/*` unit tests.
- Runtime/lifecycle: `native/src/runtime.rs` tests.
- UI/status parsing: `app/src/test`.
- Compose screens: `app/src/androidTest`.
- Real ALVR/device behaviour: `docs/manual-test-plan.md`.

`cargo test --workspace` must pass. `clippy -D warnings` and `cargo fmt
--check` are CI gates. Add evidence (logs/traces/numbers) for performance and
reliability claims.

## Gotchas

- ALVR's decoder gives an `AHardwareBuffer*`; wrapping it in an `EGLImage`
  must happen on the render thread with the EGL context current.
- `ClientCoreContext` starts its own connection thread. Call
  `ndk_context::initialize_android_context` before creating it.
- `ClientCoreContext::pause()` can block while the connection winds down; do
  not call it on the render thread.
- The Android rotation-vector timestamp is `CLOCK_BOOTTIME` nanoseconds, the
  same clock ALVR expects.
- A 2400×1080 screen is 1200×1080 per eye; ALVR encodes 1184×1056 after
  32-pixel alignment.
- No keystore, `.jks`, `keystore.properties`, token or webhook URL may enter
  the repository.
- Update docs in the same change as behaviour.

## Useful references

- `PROMPT.md`
- `docs/adr/`
- `docs/ARCHITECTURE.md`
- `docs/COMPATIBILITY.md`
- `docs/TROUBLESHOOTING.md`
- Upstream: https://github.com/alvr-org/ALVR
