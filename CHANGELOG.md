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

### Known gaps

- End-to-end streaming on a physical phone has not yet been recorded.
- Release signing and the GitHub Release workflow are not yet exercised.
- EGLImage pooling and full native render-loop ownership remain M4 work.
