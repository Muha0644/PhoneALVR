# ADR 0011: Render loop ownership

- Status: accepted
- Date: 2026-11-05

## Context

The brief forbids driving the main render loop from a `GLSurfaceView` Java GL
thread and forbids polling ALVR from the render loop.

## Decision

- The Kotlin `VrActivity` creates a `SurfaceView` and a dedicated
  `PhoneALVR-Render` thread. That thread owns the EGL context and calls a tiny
  JNI entry point once per vsync period.
- A native event thread drains ALVR's event queue; the render thread never
  polls ALVR events.
- A native input thread sends tracking and deduplicated view parameters at
  sensor rate.
- The eventual target is to move the pacing loop into Rust; the current split
  keeps the EGL ownership rules explicit and testable on device.

## Consequences

- The render thread does no ALVR event polling and no mesh generation.
- Profile changes are applied through an atomic dirty flag at frame start.
- Per-frame EGLImage creation should be replaced by a small image pool once the
  first device measurements are available; this is tracked as an M4 task.
- The render thread still takes a short statistics lock after submit; replacing
  it with atomics is an M4 optimisation. No allocation or ALVR event polling
  happens on the render thread.
