# PhoneALVR — Project Prompt & Requirements

**Status:** implementation brief (draft)
**Reference implementation:** `../PhoneVR` (the existing, working PhoneVR client)
**Audience:** the implementation agent and human reviewers

---

## 0. How to use this document

You are building a **new Android application from scratch** that turns an ordinary Android
phone into a wireless (and USB-wired) VR display for SteamVR games streamed by an **ALVR**
server. The existing repository at `../PhoneVR` is a working reference, not a code base to
extend. Read it to learn the protocol integration and the hard-won edge cases; do not copy its
architecture.

Rules for the implementation agent:

- Read the reference implementation **before** writing code. Start with:
  - `../PhoneVR/AGENTS.md` — repository guide and ALVR integration notes
  - `../PhoneVR/android/BUILD.md` — build pipeline and resolution negotiation
  - `../PhoneVR/android/app/src/main/cpp/alvr_main.cpp` — current native glue and event flow
  - `../PhoneVR/android/app/src/main/java/viritualisres/phonevr/ALVRActivity.java` — lifecycle
  - `../PhoneVR/android/prepare-alvr-deps.sh` — dependency/bootstrap steps
  - `../PhoneVR/docs/dev/` — debugging handoffs and test results
- Treat this document as the source of truth for **what** to build. For decisions marked
  `OPEN`, propose a short ADR (architecture decision record) and ask before committing to a
  large irreversible choice.
- Implement in **vertical slices**: every milestone must build, run, and pass its tests before
  moving on.
- Add tests and documentation with each feature, not at the end.
- Never commit secrets, keystores, signing passwords, or hardcoded webhook URLs.
- Prefer measured evidence (logs, traces, benchmarks) over claims in commit messages.

Requirement keywords: **MUST** = required for v1, **SHOULD** = strongly expected, **MAY** =
optional. IDs are stable so they can be referenced in issues and tests.

---

## 1. Mission

Build a stable, low-latency Android VR client for ALVR that:

1. Appears as a client to a **stock ALVR streamer** on a PC over Wi-Fi (same LAN, hotspot, or USB
   tethering) and over ALVR's native **wired/USB** mode. ALVR establishes the connection: the
   phone advertises and listens, and the streamer connects to it.
2. Presents the streamed stereo image on the phone with correct per-eye geometry, lens
   distortion, and optional Cardboard-style viewer support.
3. Provides smooth 3DoF head tracking from the phone's IMU, with correct pose prediction and
   late-latching timewarp.
4. Survives real-world use: screen off/on, network changes, app backgrounding, permission denial,
   and repeated start/stop cycles.
5. Is maintainable: small native surface area, reproducible builds, testable components,
   documented compatibility with ALVR releases.

**Definition of success:** a user installs the APK, starts ALVR on the PC, sees the phone in the
ALVR dashboard, connects, and plays for an hour without crashes, drift, or leaks. A developer can
clone, bootstrap, build, and run the test suite on a clean machine with one documented command.

---

## 2. Product principles

1. **Stability over feature count.** A smaller client that never crashes beats a feature-rich one
   that does.
2. **Own the renderer.** Do not depend on the abandoned Cardboard SDK for distortion. Implement
   the small amount of stereo/distortion/timewarp code directly, with a viewer-profile model.
3. **Thin app, strong core.** Keep Android lifecycle/UI in Kotlin and session/protocol/decode
   logic in a small, testable native core.
4. **Build once, link everywhere.** The ALVR client core is expensive to compile; build it once
   per ALVR version in CI and consume a prebuilt artifact in normal app builds.
5. **Upstream-friendly.** Prefer a pinned ALVR release or a maintained fork over ad-hoc patch
   scripts. Contribute capability changes upstream when possible.
6. **No hidden coupling.** Every external dependency is pinned, documented, and licensed.
7. **Measure latency and frame pacing.** Performance work must be driven by traces, not guesses.
8. **No secrets in the repository.** Signing keys and private endpoints live in CI secrets.

---

## 3. Reference implementation: what to reuse and what to avoid

### 3.1 Reuse as knowledge

- ALVR client-core integration: event names, decoder configuration, stream start/stop, frame
  acquisition, and the C API surface currently used by `alvr_main.cpp`.
- Tracking semantics: `alvr_send_tracking`, `alvr_send_view_params`, and the importance of not
  sending view parameters on every frame.
- Resolution negotiation: native per-eye resolution, ALVR's 32-pixel alignment, and the
  `max_view_*` capability story described in `../PhoneVR/android/BUILD.md`.
- Wired mode: ALVR connects to the app over an ADB-forwarded loopback port; the streamer
  identifies the client by package name, so the package name is part of the user-facing setup.
- Cardboard viewer-profile UX: QR scanning, lens distortion, IPD, and fallback profiles.
- Build prerequisites: Rust Android targets, `cargo-ndk`, NDK, CMake, and signing.
- Troubleshooting knowledge in `../PhoneVR/README.md` and `../PhoneVR/docs/dev/`.

### 3.2 Do not inherit these anti-patterns

- A huge ALVR submodule (including its `build/`, `target/`, and nested checkouts) committed or
  vendored into the app repo.
- Local patch files applied by a shell script to a pinned submodule as the normal workflow.
- Pinning an old `cargo-ndk` because ALVR's `xtask` uses a renamed CLI flag.
- Business/protocol logic inside JNI functions or inside `onDrawFrame`.
- Polling ALVR events from the render loop.
- The Cardboard SDK fork AAR plus manual `extractNdk` unzipping of `.so` files.
- Four ABIs compiled into every APK.
- `file(GLOB_RECURSE ...)` for native sources.
- Hardcoded crash-reporting webhooks.
- Committed keystores or signing properties with secrets.
- Emulator sensor injection through adb telnet as the primary end-to-end test.
- Fragile teardown (the old client had a JNI name typo in `destroyNative` that crashed every
  exit).
- Assuming the phone initiates the connection or discovers servers; ALVR's streamer is the TCP
  client and the phone is the listener.

---

## 4. Goals and non-goals

### 4.1 Goals (v1)

- **G1.** Accept connections from a current ALVR streamer over wireless and wired links.
- **G2.** Display a correctly distorted stereo image with per-eye viewports.
- **G3.** Smooth 3DoF tracking from the phone IMU with prediction/timewarp.
- **G4.** Clean, idempotent lifecycle and reconnection.
- **G5.** Viewer-profile support (QR scan/import/manual parameters).
- **G6.** A small settings surface: server, viewer, refresh rate, resolution cap, brightness,
  debug HUD.
- **G7.** Reproducible build + CI + automated tests.
- **G8.** Clear diagnostics: logs, stats overlay, exportable report.

### 4.2 Non-goals (v1)

- Writing a new streamer, protocol, or encoder. ALVR remains the server.
- Supporting dedicated OpenXR headsets (Quest/Pico). ALVR's official client covers those.
- Reviving the legacy Windows SteamVR driver or the GoogleVR (GVR) stack.
- 6DoF controller tracking, hand tracking, eye tracking, or full-body tracking.
- Support for Android versions below 8.0 (API 26).
- Cloud streaming or remote networks outside the LAN (out of scope; document requirements).
- A polished store presence on day one (APK/F-Droid-first is acceptable).

---

## 5. Target platform and compatibility

| Item | Requirement |
|---|---|
| Minimum Android | API 26 or 29 `MUST` |
| Target/compile SDK | Latest stable at implementation time (35+) `MUST` |
| Primary ABI | `arm64-v8a` `MUST` |
| Secondary ABI | `armeabi-v7a` `SHOULD` (only if it does not compromise schedule) |
| Emulator ABI | `x86_64` `MUST` for development/CI |
| Graphics | OpenGL ES 3.0+ `MUST`; Vulkan `MAY` later |
| Displays | 60/72/90/120 Hz panels `SHOULD` be detected and matched |
| Tracking | 3DoF from phone IMU `MUST`; SLAM/6DoF/controllers `MAY` later |
| Orientation | Landscape, sensor-landscape `MUST` |
| ALVR server | Latest stable release at implementation time `MUST`; document the exact tested version |

Do not assume a specific phone. Test on at least one physical arm64 device and one emulator.

---

## 6. Architecture

### 6.1 Repository modules

Proposed layout (adjust names, keep the separation):

```
PhoneALVR/
├── PROMPT.md                  # this document
├── README.md
├── BUILD.md
├── AGENTS.md
├── docs/
│   ├── ARCHITECTURE.md
│   ├── COMPATIBILITY.md
│   └── TROUBLESHOOTING.md
├── app/                       # Android app: Kotlin, Compose, lifecycle, UI
│   └── src/main/...
├── core/                      # Rust: ALVR session, decode, tracking, state machine
│   └── src/...
├── render/                    # GLES renderer (Rust `glow` or C++17; see OPEN-1)
├── viewer/                    # viewer profiles, QR parsing, distortion math
├── scripts/                   # bootstrap, build-core, fetch-deps
├── .github/workflows/
└── gradle/libs.versions.toml
```

The app must remain usable as a single Gradle build. Native code is built by a Gradle task that
invokes Cargo, but normal app builds `SHOULD` consume a prebuilt core artifact (see §12).
If possible, use a Makefile to make building easier.

### 6.2 Native boundary

- `OPEN-1`: choose **Rust-native** (recommended) or **C++** for the renderer/glue.
  - **Rust-native (recommended):** a `cdylib` that depends on `alvr_client_core`, exposes JNI
    with the `jni` crate, and renders with `glow` + EGL. One toolchain, no headers, direct access
    to core events.
  - **C++ alternative:** a single C ABI and one thin JNI facade. No protocol logic in JNI.
- The JNI surface `MUST` be small and stable: `create`, `start`, `stop`, `destroy`, `updateSettings`,
  `onSurfaceCreated`, `onSurfaceChanged`, `onSurfaceDestroyed`, `pollStatus`, `getStats`.
- All session state lives in a native `Session` state machine with explicit states:
  `Idle/Advertising → Incoming Connection → Handshaking → Streaming → Reconnecting → Closed`.
- Teardown `MUST` be idempotent and safe from any state. No native call may crash on a second
  `destroy`.

### 6.3 Threading model

- **Network/decode thread:** owned by the ALVR core; do not block it.
- **Event thread:** receives ALVR events and forwards them to the state machine/UI. Events are
  pushed, never polled from the render loop.
- **Input thread:** reads IMU at sensor rate, applies prediction, sends tracking.
- **Render thread:** owns the EGL context, renders at vsync, applies late-latching timewarp.
- **UI thread:** Kotlin/Compose only; observes state via `StateFlow`.
- No blocking I/O or allocations on the render thread.

### 6.4 ALVR dependency

- `OPEN-2`: choose one:
  1. **Cargo git dependency** on a pinned ALVR revision (preferred if the workspace crates are
     consumable).
  2. **Shallow submodule** pinned to a tag, without committing build outputs.
- Do not apply loose patch files at build time. If a patch is unavoidable, keep it in a fork with
  tests and document why it exists.
- The app `MUST` check the negotiated protocol/version during the streamer-initiated handshake and
  show a clear error on mismatch instead of failing silently.
- The client `MUST NOT` initiate TCP connections to the streamer; ALVR's control and stream
  connections are server-initiated.

---

## 7. Functional requirements

### 7.1 Discovery, listening, and connection establishment

- **FR-1.** First-run onboarding explains that ALVR runs on the PC and that the phone appears as a
  client in the ALVR dashboard; link to setup docs.
- **FR-2.** The app `MUST` advertise the phone as an ALVR client (the ALVR core's UDP announcement
  plus the `_alvr._tcp.local.` mDNS service where the platform permits) and `MUST` listen on the
  ALVR control/stream ports. The app `MUST NOT` dial the streamer: ALVR connections are
  server-initiated.
- **FR-3.** While idle, show a clear "Waiting for ALVR" state with the phone's hostname, LAN IP
  addresses, and listening ports, plus a copy action. Manual connection is a PC-side action: the
  UI `MUST` explain how to add the client by IP in the ALVR dashboard.
- **FR-4.** Wired mode `MUST` work with ALVR's native wired connection. The streamer forwards an
  ADB port and connects to the phone's listener; document that the ALVR
  **Wired Client Type → Custom** package name must match the app's application ID.
- **FR-5.** A deprecated manual `adb forward tcp:9943/9944` fallback `MAY` be supported and
  documented. The forward maps host ports to the phone's listening ports, and the streamer then
  connects to `127.0.0.1`.
- **FR-6.** Show actionable incoming-connection errors: no server has connected yet, protocol or
  version mismatch, port already in use, ADB unauthorized, Wi-Fi AP isolation, or a firewall
  blocking the phone's listeners.
- **FR-7.** On disconnect, keep advertising and listening so the streamer can reconnect without
  restarting the app; expose a "Stop waiting" action that stops the listener.

### 7.2 Session lifecycle

- **FR-8.** Starting/stopping streaming must be safe from the UI, notification, and system
  lifecycle callbacks.
- **FR-9.** `onPause`/`onStop`/surface destruction must not leak threads, file descriptors, or
  decoder instances.
- **FR-10.** A foreground service with a persistent notification `MUST` keep the session alive
  while streaming.
- **FR-11.** Handle screen off/on and surface recreation without losing the session where
  possible, or cleanly restart it where not.
- **FR-12.** 100 consecutive start/stop cycles `MUST` complete without leaks or crashes.

### 7.3 Tracking and input

- **FR-13.** Read IMU at the sensor's native rate; apply prediction consistent with ALVR's
  expected timestamp model.
- **FR-14.** Send view parameters (IPD/FOV) only when they change; never on a fixed high-frequency
  timer.
- **FR-15.** Support recentering/reset orientation from the UI and a hardware/volume-key shortcut
  `SHOULD`.
- **FR-16.** Respect display rotation and lock the streaming activity to landscape/inverted landscape.
- **FR-17.** `MAY` forward basic controller/keyboard input if ALVR supports it; not required for
  v1.

### 7.4 Viewer profiles and settings

- **FR-18.** Scan a Cardboard viewer QR code with CameraX/ZXing or import a profile file.
- **FR-19.** Provide a built-in default profile (Cardboard v1-equivalent) and manual entry of
  lens/IPD/screen parameters.
- **FR-20.** Persist settings per device: server, viewer profile, refresh-rate preference,
  resolution cap, brightness, debug overlay.
- **FR-21.** Show the negotiated stream resolution and the phone's native per-eye resolution in the
  settings, and warn when the stream exceeds the panel.
- **FR-22.** Settings `MUST` apply without requiring an app restart where technically possible.

### 7.5 Diagnostics

- **FR-23.** A debug HUD shows FPS, frame time, latency estimate, dropped frames, bitrate,
  resolution, codec, network state, and tracking rate.
- **FR-24.** Export a diagnostics bundle (logs, device info, settings with secrets redacted,
  stats) via the share sheet.
- **FR-25.** Logging is structured and level-controlled (debug builds verbose, release builds
  quiet).
- **FR-26.** Crash reporting is **opt-in** and configurable; no hardcoded external webhook.

### 7.6 Permissions and system integration

- **FR-27.** Camera permission only for QR scanning; microphone only if mic passthrough is
  implemented; notifications for the foreground service on Android 13+.
- **FR-28.** Do not request storage permission on Android 10+.
- **FR-29.** Handle permission denial gracefully: the app remains usable except for the affected
  feature.
- **FR-30.** Declare and handle battery-optimization/Doze guidance for long sessions.

---

## 8. Rendering requirements

- **RR-1.** Use `SurfaceView` + EGL with a dedicated render thread; do not use `GLSurfaceView`'s
  Java GL thread for the main loop.
- **RR-2.** Render a stereo pair with correct per-eye viewports, IPD, and FOV.
- **RR-3.** Implement lens distortion directly (fragment shader or precomputed mesh) from the
  viewer profile. Do not depend on the Cardboard SDK.
- **RR-4.** Support chromatic-aberration correction and optional vignette.
- **RR-5.** Implement late-latching/asynchronous timewarp using the newest IMU sample at
  presentation time.
- **RR-6.** Zero-copy decode path: decoder output → external texture/hardware buffer → GL; no
  per-frame CPU copies or `glReadPixels`.
- **RR-7.** Handle color space and brightness consistently; provide a brightness slider.
- **RR-8.** Match the display refresh rate to the streaming frame rate when the panel supports it;
  use `eglPresentationTimeANDROID`.
- **RR-9.** Survive surface loss/recreation without leaking EGL resources.
- **RR-10.** No allocations, locks, or blocking calls on the render thread in steady state.
- **RR-11.** Render a debug overlay only when enabled, without disturbing frame pacing.
- **RR-12.** Distortion math `MUST` be unit-tested (known coefficients → expected sample
  positions) and validated visually on a checkerboard/grid pattern.

---

## 9. Performance requirements

Targets are for a mid-range arm64 phone at 72–90 Hz, with a healthy Wi-Fi 5/6 link:

- **PR-1.** App-added motion-to-photon latency ≤ 1 vsync beyond the core/decoder baseline;
  end-to-end target ≤ 80 ms measured with a high-speed camera or ALVR statistics.
- **PR-2.** Dropped frames < 1% over a 10-minute session, excluding network/encoder stalls.
- **PR-3.** Render-thread frame time < 8 ms at 90 Hz, < 11 ms at 72 Hz.
- **PR-4.** No steady-state allocations on the render thread; no GC pauses during streaming.
- **PR-5.** Memory growth < 5% over a 60-minute soak.
- **PR-6.** Time from the streamer connecting to the first rendered frame < 5 s on a healthy LAN,
  excluding server startup.
- **PR-7.** Release APK per ABI < 40 MB (excluding optional debug symbols), or document the size.
- **PR-8.** A clean app build with a prebuilt core < 10 minutes; full CI < 25 minutes.
- **PR-9.** Provide a repeatable benchmark/trace recipe (Perfetto, `adb shell dumpsys gfxinfo`,
  ALVR logs) and record results per milestone.

---

## 10. Reliability requirements

- **REL-1.** 60-minute continuous streaming soak: no crash, ANR, or unbounded memory growth.
- **REL-2.** Remain discoverable and listening after network loss so the streamer can reconnect
  within 5 seconds of the network returning.
- **REL-3.** Safe teardown from every state, including "server disappears mid-frame".
- **REL-4.** No `UnsatisfiedLinkError`, JNI name mismatch, or use-after-free on exit.
- **REL-5.** Handle malformed/partial decoder configuration without crashing.
- **REL-6.** Handle server version mismatch with a clear message and a documented compatibility
  matrix.
- **REL-7.** Handle low-memory and thermal-throttling conditions gracefully (reduce quality or
  warn) where the core exposes the necessary signals.

---

## 11. ALVR integration requirements

- **ALVR-1.** Integrate the ALVR client core as a pinned dependency; record the exact version/revision
  in `docs/COMPATIBILITY.md`.
- **ALVR-2.** Support the stream settings chosen on the server: resolution, refresh rate, bitrate,
  codec, foveation, and reprojection options that the core exposes.
- **ALVR-3.** Advertise the phone's real display capabilities (per-eye resolution, refresh rates,
  FOV, IPD) using released ALVR capabilities. Prefer upstream support over local patches.
- **ALVR-4.** Respect ALVR's resolution alignment (multiples of 32) and never assume the requested
  resolution equals the encoded resolution.
- **ALVR-5.** Deduplicate view/parameter updates; do not trigger server-side distortion rebuilds.
- **ALVR-6.** Document wired-mode setup, including the custom client package name and autolaunch.
- **ALVR-7.** Fail fast and clearly on protocol/version mismatch during the server-initiated
  handshake; never silently hang.
- **ALVR-8.** Do not bundle ALVR server components or SteamVR/OpenVR binaries.
- **ALVR-9.** Treat the phone as the TCP listener: it advertises and accepts connections; it never
  dials the streamer.

---

## 12. Build, tooling, and CI

- **BUILD-1.** One-command bootstrap (`scripts/bootstrap.sh` or equivalent) installs/verifies:
  JDK 17, Android SDK/NDK, Rust targets, `cargo-ndk`, and CMake.
- **BUILD-2.** Pin all toolchains: Gradle wrapper, AGP, Kotlin, NDK, Rust via
  `rust-toolchain.toml`, and the ALVR revision.
- **BUILD-3.** Build the native ALVR core **once per version/ABI** and publish it as an artifact;
  the normal app build consumes it. Provide a `build-core` task for developers who need to rebuild.
- **BUILD-4.** Use **prefab** or a plain `jniLibs` directory for native dependencies. Do not
  unzip `.so` files out of AARs at configuration time.
- **BUILD-5.** Explicit native source lists; no `GLOB_RECURSE` for sources.
- **BUILD-6.** ABI splits / App Bundle. Default release: `arm64-v8a`; add `x86_64` only for
  emulator/dev builds.
- **BUILD-7.** Use Gradle version catalogs and convention plugins; no version literals scattered
  across files.
- **BUILD-8.** Debug signing for PR builds; release keystore from CI secrets only. No `.jks`,
  `keystore.properties`, or passwords in the repository.
- **BUILD-9.** Cache Gradle, Cargo, NDK, and the prebuilt core in CI.
- **BUILD-10.** Reproducible builds: no build-time downloads in the normal app path; vendor or
  checksum-pin anything fetched.
- **BUILD-11.** Enforce formatting/lint: `cargo fmt`/`clippy`, ktlint/Spotless, Android Lint,
  clang-format if C++ is used.
- **BUILD-12.** Provide `make`/Gradle aliases for the common tasks: `bootstrap`, `build`,
  `test`, `lint`, `format`, `core`.

Suggested versions are intentionally not pinned here; use current stable releases and record them
in `gradle/libs.versions.toml`, `rust-toolchain.toml`, and `docs/COMPATIBILITY.md`.

---

## 13. Testing and verification

- **TEST-1.** Host unit tests for the session state machine, settings parsing, and viewer-profile
  math.
- **TEST-2.** Rust unit/integration tests for the core boundary (no Android required).
- **TEST-3.** A fake transport/mock streamer so connection, decoder config, and teardown can be
  tested without SteamVR.
- **TEST-4.** Instrumented tests on an emulator (`x86_64`) for launch, permissions,
  incoming-connection failure paths, and lifecycle cycling.
- **TEST-5.** Compose UI tests / screenshot tests for the main screens.
- **TEST-6.** A documented manual test plan for real ALVR + Wi-Fi + wired sessions.
- **TEST-7.** Soak test (60 min) and 100× start/stop cycle test, run before release.
- **TEST-8.** Performance regression check using the benchmark recipe from `PR-9`.
- **TEST-9.** CI runs lint, unit tests, core tests, app build, and emulator tests on every PR.
- **TEST-10.** Emulator sensor injection may be used for convenience, but `MUST NOT` be the only
  correctness test for tracking/rendering.

---

## 14. Security, privacy, and licensing

- **SEC-1.** No secrets, tokens, webhooks, keystores, or signing passwords in the repository.
- **SEC-2.** Crash/telemetry reporting is opt-in, documented, and points to a configurable
  endpoint. Redact device identifiers by default.
- **SEC-3.** Request only the permissions needed for enabled features; explain each in the UI.
- **SEC-4.** Document that ALVR's LAN protocol is not transport-encrypted by default; recommend a
  trusted network or wired/USB mode.
- **SEC-5.** Track licenses for ALVR (MIT), Cardboard-derived code (Apache-2.0), and every native
  dependency. Include a `NOTICE`/licenses screen `SHOULD`.
- **SEC-6.** Validate QR/profile input; never execute or evaluate imported data.

---

## 15. Documentation requirements

- **DOC-1.** `README.md`: what it is, requirements, install, adding the phone in ALVR, connect
  (wireless + wired), troubleshooting, compatibility matrix, links.
- **DOC-2.** `BUILD.md`: prerequisites, bootstrap, build, test, release, core rebuild.
- **DOC-3.** `docs/ARCHITECTURE.md`: modules, threading, state machine, rendering pipeline,
  native boundary.
- **DOC-4.** `docs/COMPATIBILITY.md`: tested ALVR server versions, phone models, Android versions,
  known limitations.
- **DOC-5.** `docs/TROUBLESHOOTING.md`: connection failures, black screen, distortion issues,
  latency, logs.
- **DOC-6.** `CONTRIBUTING.md`: coding standards, commit style, test expectations.
- **DOC-7.** `AGENTS.md`: practical guide for coding agents (build commands, layout, gotchas).
- **DOC-8.** `CHANGELOG.md`: user-visible changes per release.

Documentation `MUST` be updated in the same change as the behavior it describes.

---

## 16. Suggested milestones

Each milestone ends with a demo and passing tests.

### M0 — Skeleton and first frame

- Gradle project + Rust core crate compile on Linux/CI.
- ALVR dependency resolved and pinned.
- Accept a connection from a real ALVR streamer, receive one decoded frame, render it to a
  full-screen quad (no distortion), and log stats.
- Basic CI: lint + build + unit tests.
- **Exit:** first frame on a physical device; no crashes on teardown.

### M1 — Playable stereo + tracking

- Stereo viewports, correct IPD/FOV, basic barrel distortion from a default profile.
- IMU tracking at sensor rate, view-parameter deduplication, recenter.
- 10-minute session without leaks.
- **Exit:** playable session in a Cardboard viewer on a physical phone.

### M2 — Comfort and configuration

- Late-latching timewarp, chromatic-aberration correction, refresh-rate matching.
- Viewer-profile QR scan + manual profile; settings persistence.
- Wired mode + reconnect.
- **Exit:** wired and wireless sessions both pass the manual test plan.

### M3 — Reliability and diagnostics

- Foreground service, surface recreation, permission-denial paths.
- Debug HUD, diagnostics export, opt-in crash reporting.
- Soak test, 100× start/stop, reconnect test.
- **Exit:** reliability requirements pass on at least two devices/one emulator.

### M4 — Performance and release

- Benchmark and tune to the performance targets.
- ABI splits, signing via CI secrets, release workflow, docs.
- **Exit:** release candidate APK + compatibility matrix + green CI.

---

## 17. Definition of done (v1)

- [ ] Accepts connections from the pinned ALVR streamer over Wi-Fi and native wired mode.
- [ ] Stereo image is correctly distorted for at least the default and one scanned viewer profile.
- [ ] 3DoF tracking is smooth; no view-param spam; recenter works.
- [ ] 60-minute soak, 100× start/stop, and network-reconnect tests pass.
- [ ] No crashes/ANRs in the documented test matrix.
- [ ] Performance targets measured and recorded; regressions have a benchmark.
- [ ] CI is green: lint, unit, core, build, emulator tests.
- [ ] Release APK is signed from CI secrets; no secrets in the repo.
- [ ] README, BUILD, ARCHITECTURE, COMPATIBILITY, and TROUBLESHOOTING are complete.
- [ ] Licenses/NOTICE are included.
- [ ] An ADR exists for every `OPEN` decision.

---

## 18. Open decisions (ask before implementing)

- **OPEN-1.** Native implementation: all-Rust core + `glow` renderer (recommended) vs. C++17
  renderer with a thin C ABI.
- **OPEN-2.** ALVR dependency: Cargo git dependency vs. shallow submodule.
- **OPEN-3.** Application ID and display name. Recommendation: a new ID (e.g.
  `dev.phonealvr.client`) so it can coexist with the old app; document the wired-client package
  name. Confirm the final ID.
- **OPEN-4.** Minimum SDK: 26 (Android 8.0) vs. 28 (Android 9). Recommendation: 26 unless a
  required API forces 28.
- **OPEN-5.** Distortion implementation: fragment shader vs. precomputed mesh. Recommendation:
  mesh for arbitrary lens profiles, shader for speed; benchmark both.
- **OPEN-6.** Whether mic passthrough and controller input are in v1 or deferred.
- **OPEN-7.** Distribution: GitHub Releases APK, F-Droid, Play Store, or all. Affects signing,
  ABI splits, and metadata.
- **OPEN-8.** Upstream strategy: contribute a Cardboard backend to ALVR, or keep the client
  standalone.
- **OPEN-9.** Viewer-profile format compatibility: reuse Cardboard's JSON schema (recommended) or
  define a new one.
- **OPEN-10.** Whether to support secondary ALVR server versions via a compatibility shim.

---

## 19. Agent workflow rules

1. **Read first, then propose.** Produce a short architecture plan and ADRs for the `OPEN`
   decisions before writing large amounts of code.
2. **Vertical slices.** Each change should compile, run, and be testable. Do not leave the tree
   broken between milestones.
3. **No silent scope creep.** If a requirement is ambiguous or appears wrong, ask or record an
   ADR; do not guess silently.
4. **Prefer deletion over abstraction.** Do not add a framework or dependency unless it removes
   more complexity than it adds.
5. **Pin and document.** Every dependency, toolchain, and ALVR revision gets an explicit version
   and a short rationale.
6. **Test the risky parts.** Lifecycle, teardown, threading, and distortion math need automated
   tests before they are considered done.
7. **Evidence over claims.** Include logs, traces, or benchmark numbers in the PR/commit for
   performance or reliability work.
8. **Keep the repo clean.** No generated build outputs, no vendored binaries without a license and
   checksum, no secrets, no dead code.
9. **Update docs with behavior.** A change that alters setup, compatibility, or UX updates the
   relevant doc in the same change.
10. **Ask before irreversible moves.** Large renames, history rewrites, dependency swaps, and
    release/signing changes require confirmation.

---

## 20. Reference quick links (old repo)

- `../PhoneVR/AGENTS.md`
- `../PhoneVR/README.md`
- `../PhoneVR/android/BUILD.md`
- `../PhoneVR/android/app/src/main/cpp/alvr_main.cpp`
- `../PhoneVR/android/app/src/main/java/viritualisres/phonevr/ALVRActivity.java`
- `../PhoneVR/android/prepare-alvr-deps.sh`
- `../PhoneVR/docs/dev/STEAMVR_HANDOFF.md`
- `../PhoneVR/docs/dev/feedback.md`
- ALVR upstream: https://github.com/alvr-org/ALVR
