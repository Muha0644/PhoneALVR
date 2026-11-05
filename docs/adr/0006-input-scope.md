# ADR 0006: Input scope for v1

- Status: accepted
- Date: 2026-11-05

## Decision

Ship 3DoF head tracking only. Microphone passthrough, controller forwarding and
hand/eye tracking are deferred. A missing microphone permission must not affect
the session.

## Consequences

- No microphone permission is requested.
- The input surface is the Android rotation-vector sensor, fed to the native
  tracker. Volume-down and a screen tap recenter the view.
- `FR-17` remains a MAY and is out of scope for v1.
