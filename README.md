# PhoneALVR

Turn an ordinary Android phone into a wireless (or USB-wired) VR display for
SteamVR games streamed by an **ALVR** server.

PhoneALVR is a new, deliberately small client. It is **not** a fork of
PhoneVR and does **not** depend on the Cardboard SDK: viewer profiles, lens
distortion, stereo geometry, tracking prediction and the GLES renderer are
implemented in this repository.

> **Status: implementation in progress.** The Rust core, viewer math, session
> state machine, renderer and JNI boundary are implemented. The host test suite
> (94 Rust tests), Android cross-compilation for arm64/x86_64, Kotlin unit
> tests, Android Lint and debug/release APK builds all pass; see
> [`docs/VERIFICATION.md`](docs/VERIFICATION.md). End-to-end validation against a
> physical ALVR streamer and a physical phone is the next milestone; see
> `docs/COMPATIBILITY.md` for exactly what has and has not been exercised.

## What it does

- The phone advertises itself as an ALVR client and **listens**. ALVR's PC
  streamer is the TCP client; the phone never dials the PC.
- Decoded H.264/HEVC/AV1 frames from ALVR's MediaCodec decoder go directly to
  an `AHardwareBuffer` → `EGLImage` → GL external texture (zero-copy).
- A stereo distortion mesh is generated from a Cardboard-compatible viewer
  profile and rendered with brightness, vignette and optional chromatic
  aberration correction.
- 3DoF head tracking comes from Android's rotation-vector sensor. The native
  tracker estimates angular velocity, predicts a short horizon for late
  latching, and recenters on demand.
- View parameters are sent only when the viewer/IPD/FOV actually changes, not
  at tracking rate. This avoids the ALVR 20.12+ `ViewsConfig` packet storm and
  SteamVR warp-mesh rebuilds.
- A foreground service keeps the listener and session alive while the screen is
  off or another app is in front.

## Requirements

| Item | Requirement |
| --- | --- |
| Phone | Android 8.0+ (API 26), OpenGL ES 3.0, arm64-v8a |
| ALVR server | **v20.14.1** (pinned client core; see `docs/COMPATIBILITY.md`) |
| PC | ALVR + SteamVR, same LAN as the phone (or ADB for wired mode) |
| Viewer | Any Cardboard-style viewer; built-in Cardboard v1 fallback |

## Build from source

Prerequisites: JDK 17, Android SDK, Android NDK r27, Rust (pinned by
`rust-toolchain.toml`) and `cargo-ndk 3.5.4`.

```bash
./scripts/bootstrap.sh          # verify/install toolchains
make core                       # build libphonealvr.so for arm64-v8a
make build                      # assemble the debug APK
make install                    # adb install -r the debug APK
```

On a clean machine, `make core` needs the NDK and may take a while the first
time because ALVR's client core is compiled. Normal app builds consume the
prebuilt `app/src/main/jniLibs` and do not invoke Cargo.

See [`BUILD.md`](BUILD.md) for the full build, test, emulator and release
instructions.

## Connect

### Wireless (same Wi-Fi / hotspot)

1. Start ALVR on the PC.
2. Put the phone and PC on the same network. Guest Wi-Fi with AP isolation
   will not work.
3. Open PhoneALVR and press **Start waiting**.
4. On the PC, ALVR → **Connection** → the phone should appear in the device
   list. Trust it.
5. When the headset view starts streaming, press **Open VR** (or launch the VR
   activity from the notification).
6. Put the phone into the viewer. Tap the status overlay or press volume-down
   to recenter.

The waiting screen shows the phone hostname, LAN IPs and ports `9943`
(control) and `9944` (stream). If discovery does not find the phone, add it by
LAN IP in ALVR: **Connection → Add device → IP**.

### Wired / USB (ALVR native wired mode)

ALVR v20.12+ supports a native wired connection. The streamer forwards an ADB
port and connects to the phone's listener; the phone still does not dial the
PC.

1. Enable USB debugging and authorize the PC.
2. In ALVR: **Connection → Wired Client Type → Custom** and set the package
   name to `dev.phonealvr.client` (`dev.phonealvr.client.debug` for a debug
   build). Optionally enable **Wired Client Autolaunch**.
3. Connect the USB cable and start waiting in PhoneALVR.

The deprecated manual fallback still works:

```bash
adb forward tcp:9943 tcp:9943
adb forward tcp:9944 tcp:9944
```

Then select **TCP** and `127.0.0.1` in ALVR.

### Controls

- **Tap the status overlay** or press **volume-down**: recenter the view.
- **Start waiting / Stop waiting** in the app.
- **Stop waiting** in the persistent notification.
- The VR activity is locked to sensor-landscape and keeps the screen awake.

### Viewer profiles

A built-in Cardboard v1 profile is used until you scan a viewer QR code. The
scanner accepts modern Cardboard QR URLs (protobuf `?p=` payloads) and
Cardboard JSON profiles. The profile is persisted per device and can be reset
to the built-in default in Settings.

## Safety, privacy and licensing

- ALVR's LAN protocol is **not transport-encrypted** by default. Prefer a
  trusted network or the wired/USB mode.
- Crash reporting is **opt-in** and uses a user-configured HTTPS endpoint. No
  endpoint, token, keystore or password is hardcoded or committed.
- The app requests camera permission only for QR scanning and notification
  permission for the foreground service. It does not request storage on
  Android 10+.
- PhoneALVR is MIT licensed. ALVR is MIT licensed; the Cardboard profile schema
  is Apache-2.0. See `LICENSE` and `NOTICE`.

## Documentation

- [`BUILD.md`](BUILD.md) — build, test, release, core rebuild
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — modules, threads, state machine, renderer
- [`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md) — tested versions and limitations
- [`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.md) — connection, black screen, latency
- [`docs/VERIFICATION.md`](docs/VERIFICATION.md) — what has actually been built and tested
- [`docs/manual-test-plan.md`](docs/manual-test-plan.md) — device/soak/reconnect checklist
- [`docs/adr/`](docs/adr/) — architecture decisions, including every OPEN item
- [`AGENTS.md`](AGENTS.md) — repository guide for coding agents

## Why the phone listens

This is the most common integration mistake. ALVR's dashboard is the TCP
client: it discovers or is told about the phone, then connects to the phone's
listener. Any design where the phone tries to discover and dial the PC will
fail with stock ALVR.
