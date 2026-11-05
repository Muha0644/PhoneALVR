# ADR 0002: Pin ALVR as a Cargo git dependency

- Status: accepted
- Date: 2026-11-05

## Decision

Depend on `alvr_client_core` (and the `alvr_common` / `alvr_packets` types it
needs) from the upstream `alvr-org/ALVR` repository, pinned to tag
**v20.14.1** (commit `a9f6542fa507a841f40ab4f3fcb531427cd02550`).

Do not vendor the ALVR tree, its `target/` or `build/` directories, and do not
apply local patch files at build time. If a change is unavoidable, carry it in
a maintained fork with tests and document it in `docs/COMPATIBILITY.md`.

The normal Android app build consumes prebuilt `jniLibs` produced by
`scripts/build-core.sh`; Cargo is not invoked by Gradle. The core is therefore
compiled once per ALVR version/ABI, as required by BUILD-3.

## Why not the C API?

The C API exported by ALVR is stable but requires hand-written or generated
bindings and a second native library. The Rust API gives direct access to
events, typed view parameters and the decoder split API with no header drift.

## Consequences

- `Cargo.lock` is committed and acts as the checksum pin for the whole graph.
- Bumping ALVR requires updating this ADR, `docs/COMPATIBILITY.md`, the
  workflows and the decompression/decoder smoke tests.
- CI caches the Cargo registry and the Android core build.
