# ADR 0007: Distribution

- Status: accepted
- Date: 2026-11-05

## Decision

Distribute GitHub Releases APKs first, with a documented F-Droid path later.
Play Store packaging is not a v1 requirement. Release APKs are `arm64-v8a`
only; debug APKs add `x86_64` for the emulator. Signing keys live only in CI
secrets.

## Consequences

- The release workflow uploads an APK and its checksum.
- No keystore, password or signing properties exist in the repository.
- F-Droid metadata can be added without changing the native build.
