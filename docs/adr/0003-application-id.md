# ADR 0003: Application ID and display name

- Status: accepted; revised 2026-11-05 to use one package for debug and release
- Date: 2026-11-05

## Decision

- Display name: **PhoneALVR**
- Application ID: **`dev.phonealvr.client`**
- Debug and release use the **same** application ID, `dev.phonealvr.client`.
  The debug build keeps only a `-debug` version-name suffix. This is required
  because ALVR wired mode uses the package name to identify and autolaunch the
  client.
- Wired-client package name for ALVR: `dev.phonealvr.client`.

A new ID avoids colliding with the existing `viritualisres.phonevr` client and
makes the wired-mode setup explicit.

## Consequences

Changing the ID later invalidates Android upgrades and wired-mode
configuration. Debug and release share the package but are signed with
 different keys; uninstall one before installing the other if Android rejects
the update.
