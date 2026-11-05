# ADR 0001: Rust-native core and renderer

- Status: accepted (provisional until the first device demo)
- Date: 2026-11-05

## Context

The reference PhoneVR client uses C++ glue with the Cardboard SDK and polls ALVR
events from the render loop. The brief recommends a Rust-native `cdylib` that
depends on `alvr_client_core`, exposes JNI with the `jni` crate, and renders
with `glow` + EGL.

## Decision

Use a single Rust workspace for the platform-independent policy (`core`),
viewer math (`viewer`), GLES renderer (`render`) and JNI/ALVR boundary
(`native`). The native crate is a `cdylib` named `libphonealvr.so`.

The Kotlin app owns only Android lifecycle, permissions, Compose UI, the
foreground service, sensor registration and the Surface. Session policy,
tracking prediction, view-param deduplication, rendering and protocol handling
are in Rust.

## Consequences

- One toolchain for the core and renderer; no C++ headers, no ABI shim.
- The JNI surface stays small and stable.
- The ALVR dependency pulls wgpu transitively even though we do not use
  `alvr_graphics`; this is accepted because it is the supported client-core
  dependency graph and the core is built once per ABI.
- Android-specific code cannot be exercised by host tests; the policy is kept
  in `runtime.rs` and covered by host tests instead.
