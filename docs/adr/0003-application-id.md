# ADR 0003: Application ID and display name

- Status: accepted provisionally; confirm before the first public release
- Date: 2026-11-05

## Decision

- Display name: **PhoneALVR**
- Application ID: **`dev.phonealvr.client`**
- Debug suffix: `.debug` (so a debug build coexists with a release build)
- Wired-client package name for ALVR: the application ID above
  (`dev.phonealvr.client`, or `dev.phonealvr.client.debug` for a debug APK).

A new ID avoids colliding with the existing `viritualisres.phonevr` client and
makes the wired-mode setup explicit.

## Consequences

Changing the ID later invalidates Android upgrades and wired-mode
configuration. Confirm this choice before publishing.
