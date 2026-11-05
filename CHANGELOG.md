# Changelog

All notable user-visible changes are recorded here. The project follows
Semantic Versioning once a 1.0 release exists.

## [0.1.0] — unreleased

### Added

- Rust workspace: `core` session/settings/tracking/stats, `viewer` Cardboard
  profiles + distortion math, `render` GLES stereo pipeline, `native` JNI/ALVR
  boundary.
- Session state machine with idempotent stop/destroy, reconnect and 100-cycle
  tests.
- Cardboard viewer-profile parsing: JSON schema and QR protobuf payloads.
- Precomputed distortion mesh, chromatic aberration and vignette shader path.
- 3DoF tracking with angular-velocity estimation, clamped prediction and
  recentering.
- View-parameter deduplication to prevent ALVR `ViewsConfig` spam.
- Android app scaffold: Compose home/settings, foreground service, SurfaceView
  render thread, rotation-vector sensor input, QR scanning, diagnostics export.
- Build tooling: `scripts/bootstrap.sh`, `scripts/build-core.sh`, Makefile,
  Gradle wrapper, CI workflows.
- Documentation: README, BUILD, architecture, compatibility, troubleshooting,
  manual test plan and ADRs for the OPEN decisions.

### Changed

- Debug and release now share the application ID `dev.phonealvr.client` so ALVR
  wired mode/autolaunch identifies the client consistently.
- The listener starts automatically when the app is resumed; ALVR wired
  autolaunch no longer leaves the app idle.
- Settings moved to the top bar; diagnostics is now an action inside the
  settings dialog. Removed the duplicate session-settings card, the refresh-rate
  selector, and the copy-addresses button.
- The connection card shows the device model instead of the misleading
  `localhost` hostname.
- Decoder configuration now passes the real encoded size to MediaCodec instead
  of ALVR's 512x1024 placeholder, fixing the `ErrorUnknown` reconnect loop on
  some phones.

### Added

- Foveated encoding: ALVR v20.14.1 FFE constants and UV mapping in
  `viewer/src/foveated.rs` with unit tests, FFE config extraction and encoded
  MediaCodec sizing in the Android backend, and the renderer shader hookup
  (centre/edge remap, right-eye mirror, zero-coefficient fallback). The client
  still advertises no foveation until the shader is verified on the device.
- Real dropped-frame counter in the HUD/stats.
- Decoder configuration is now idempotent: ALVR resends the same config on
  keyframes, and recreating MediaCodec each time leaked an ImageReader and
  caused queue overflow plus `Decoder saturation` drops. The decoder is only
  rebuilt when the config actually changes.
- Settings now shows the build ID (short git hash, `-dirty` when the tree is
  modified) and the UTC build time.
- Head tracking now converts Android rotation-vector samples from the device
  (natural portrait / East-North-Up) frame into OpenVR's Y-up, -Z-forward
  frame, with the display rotation applied in the same direction as the drawn
  UI. This fixes inverted yaw/pitch when the phone is used in landscape.
- New Settings → Screen orientation option (Auto, portrait, landscape, reverse
  variants) that locks the home and VR activities to the chosen orientation.
- The render thread is now paced solely by `eglSwapBuffers` vsync instead of a
  software timer plus vsync, which was halving frame consumption and backing up
  the decoder. Empty frames are no longer swapped.
- The renderer logs presented FPS, input FPS and dropped packets every five
  seconds, and warns once when the streamer is not frame-paced. A 144 Hz
  desktop without ALVR's "Enforce server frame pacing" was sending ~143 fps to
  a 60 Hz phone, saturating the decoder and starving every frame of bitrate.
- Stream warning when the encoded side-by-side width exceeds the common 1920 px
  H.264 decoder limit.

### Known gaps

- Foveated-encoding physical-device validation is pending; the client still
  advertises no foveation.
- End-to-end streaming on a physical phone has not yet been recorded.
- Release signing and the GitHub Release workflow are not yet exercised.
- EGLImage pooling and full native render-loop ownership remain M4 work.
