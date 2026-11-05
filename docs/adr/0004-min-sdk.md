# ADR 0004: Minimum Android version

- Status: accepted
- Date: 2026-11-05

## Decision

`minSdk = 26` (Android 8.0), `targetSdk = 35`, `compileSdk = 35`.

API 26 covers the required foreground-service, notification, EGL and
MediaCodec behaviour used by the client. No supported feature needs API 28+;
the app degrades gracefully on older foreground-service type handling.

## Consequences

- `java.time` is available without desugaring.
- Notification channels are mandatory, which the service already creates.
