# Architecture

## Modules

```
PhoneALVR/
├── app/        Kotlin + Compose: lifecycle, permissions, UI, service, sensor, Surface
├── native/     Rust cdylib: JNI facade, session runtime, ALVR/EGL/MediaCodec backend
├── core/       Pure Rust: session state machine, settings, tracking, stats, dedup
├── viewer/     Pure Rust: Cardboard JSON/protobuf parsing, distortion, mesh, projection
├── render/     Rust + glow: GLES stereo renderer, EGL context, external textures
├── scripts/    bootstrap, core build, test
└── docs/       architecture, compatibility, troubleshooting, ADRs, manual tests
```

`core`, `viewer` and `render` have no Android dependency and are covered by
host tests. `native` compiles the platform backend only for
`target_os = "android"`; host builds use a no-op platform hook.

## Threads

| Thread | Owner | Responsibility |
| --- | --- | --- |
| ALVR connection/decode | `alvr_client_core` | discovery, UDP announcement, control/stream sockets, MediaCodec |
| Event | `native` | `poll_event()` loop → state machine, decoder configuration |
| Input | `native` | read tracker, `send_tracking`, deduplicated `send_view_params` |
| Render | `VrActivity` (`PhoneALVR-Render`) | EGL context, GLES draw, swap, presentation timestamp |
| UI | Kotlin/Compose | StateFlow snapshots, user actions, permissions |

The render thread never polls ALVR events. The event thread never touches GL.
Tracking samples arrive from Android's `SensorManager` at sensor rate through a
small JNI call; the input thread reads the resulting predicted pose.

## Session state machine

`core::session::Session` is pure and exhaustively tested:

```
Idle
  └─ Start ──────────────► Advertising
                              │ ServerConnected
                              ▼
                       IncomingConnection
                              │ HandshakeStarted / HandshakeSucceeded
                              ▼
                         Handshaking
                              │ StreamStarted
                              ▼
                          Streaming
                              │ StreamStopped / NetworkLost / Error
                              ▼
                    Advertising / Reconnecting
                              │ Stop
                              ▼
                            Idle
   any ── Close ──────────► Closed ── Reset ──► Idle
```

- `Close` and `Stop` are idempotent and safe from every state.
- A duplicate TCP accept is ignored.
- A handshake failure reports a clear error and keeps listening.
- A disconnect keeps the listener open so ALVR can reconnect without an app
  restart.
- Surface loss pauses rendering but does not end the session.

`native::Runtime` wraps the state machine with settings, the viewer profile,
tracking, view-parameter deduplication and statistics. This is the layer host
tests exercise for 100-cycle and reconnect behaviour.

## Native boundary

The JNI surface is intentionally small:

```text
nativeCreate(displayWidth, displayHeight, refreshHz, androidContext)
nativeDestroy, nativeStart, nativeStop
nativeUpdateSettings(json), nativeSetViewerProfile(json)
nativeOnSurfaceCreated(Surface), nativeOnSurfaceChanged(w,h), nativeOnSurfaceDestroyed
nativeRenderFrame, nativeRecenter
nativePushOrientation(x,y,z,w,timestampNs,displayRotation)
nativePollStatus, nativeGetStats, nativeGetDiagnostics
```

No protocol or business logic is implemented inside a JNI function. JNI only
copies values and delegates to `Runtime` / `platform`.

## Decode and render path

1. ALVR receives a video packet and hands NAL units to the configured decoder
   callback.
2. `alvr_client_core::video_decoder` decodes into MediaCodec and exposes an
   `AHardwareBuffer*` through `VideoDecoderSource::get_frame`.
3. The render thread wraps the buffer in an `EGLImage`
   (`eglGetNativeClientBufferANDROID` + `eglCreateImageKHR`) and binds it to a
   `GL_TEXTURE_EXTERNAL_OES` texture. No CPU copy.
4. ALVR's decoded frame contains the two eyes side by side. The per-eye
   distortion mesh produces eye-local UVs; a uniform selects the left or right
   half of the source texture.
5. A precomputed per-eye distortion mesh is drawn into the left/right viewport.
6. `eglSwapBuffers` (swap interval 1) paces the render thread; there is no
   second software timer. When no decoded frame is ready, the last frame stays
   on screen and the thread polls again instead of swapping an empty buffer.
7. ALVR receives `report_compositor_start` / `report_submit` for its statistics.
   The renderer logs measured FPS every five seconds.

### Foveated encoding

ALVR can encode a smaller per-eye texture with a sharp centre and compressed
edges. `viewer/src/foveated.rs` ports the ALVR v20.14.1 constants and UV
mapping. When FFE is negotiated, `native/src/platform_android.rs` uses the FFE
**encoded** per-eye resolution for the MediaCodec width/height instead of the
base stream resolution, and `Runtime` stores the layout for the renderer.

The renderer applies the FFE remap after lens distortion and before the
side-by-side half selection, including ALVR's right-eye horizontal mirror.
The constants are uploaded once per stream/profile change via
`StereoRenderer::set_foveated`, never per frame. `ClientCapabilities.
foveated_encoding` stays false until the shader path is verified on the
device; see `docs/dev/FFE_HANDOFF.md`.

The current implementation creates one `EGLImage` per decoded frame. Moving to
a small image pool is an M4 performance task; the decode path is already
zero-copy.

## Distortion and stereo geometry

- Viewer profile → `EyeFov` + radial coefficients.
- `TanFov` converts between normalized viewport coordinates and tangent-angle
  coordinates.
- `LensDistortion` evaluates `p' = p (1 + K1 r² + …)`.
- `EyeMapper` maps an output pixel to the source texture coordinate.
- `build_eye_mesh` samples the mapper on a 40×32 grid; the mesh is rebuilt only
  when the profile or display changes.
- The two eyes have separate meshes. Their lens centres are computed from the
  panel's physical width (`DisplayMetrics.xdpi`) and the profile's lens
  spacing, because the lenses are normally inset from the half-screen centres.
  The source image centre maps to the lens centre, so the virtual image centres
  sit under the lenses.
- Projection matrices are asymmetric and derived from the same FOV.

The math is unit-tested with known Cardboard coefficients and with mesh
topology/corner checks. A checkerboard scene validates it visually on device.

## Tracking

Android's `TYPE_ROTATION_VECTOR` sensor provides a quaternion already fused
from gyro/accelerometer/magnetometer, but it is expressed in the device's
natural portrait frame relative to an East-North-Up world. A JNI-side call to
`core::tracking::android_orientation_to_openvr` applies the display rotation
(the phone is normally used in landscape; the UI rotation is a positive Z
rotation relative to the device) and the ENU → OpenVR world change
before the sample reaches `HeadTracker`. With the screen facing the eyes, the
displayed frame is the headset frame.

`core::tracking::HeadTracker`:

- normalizes and ingests samples with monotonic timestamps;
- estimates world-frame angular velocity from the shortest-arc delta;
- predicts a clamped horizon (default 100 ms max) for late latching;
- applies a recenter offset;
- exposes angular velocity for ALVR's own prediction.

The ALVR client core also predicts using the supplied angular velocity; our
short-horizon prediction is for the final reprojection step. At presentation
time the renderer compares the newest predicted orientation with the pose ALVR
used for the decoded frame and applies a small, clamped 2D UV shift
(yaw/pitch). This is translation-only late latching, not a depth-aware
timewarp; a full warp is future work.

## Settings and persistence

Kotlin stores user settings in `SharedPreferences` and sends a JSON snapshot to
`Runtime::set_settings_json`. The Rust side validates/clamps every value
(brightness, refresh, resolution cap, profile size, crash endpoint) before it
reaches the renderer or ALVR capabilities. The viewer profile is parsed and
validated in Rust; the JSON/QR original is persisted so it survives a restart.

## Failure handling

- Port in use, ADB unauthorized, AP isolation and firewall problems surface as
  a session error plus actionable UI text.
- A protocol mismatch produces a handshake error and leaves the listener open.
- Native `destroy` is idempotent; `Close` and `Stop` are no-ops when already in
  the target state.
- A missing viewer profile uses the built-in Cardboard v1 profile.
- Malformed/partial decoder configuration is reported rather than crashing;
  the core reports fatal decoder errors back to ALVR, which reconnects.
