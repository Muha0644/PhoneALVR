# ADR 0005: Distortion implementation

- Status: accepted
- Date: 2026-11-05

## Decision

Precompute a triangle mesh on the CPU when the viewer profile or display
changes, then sample the decoded texture with the mesh UVs. The fragment shader
adds brightness, vignette and lateral chromatic-aberration offsets.

A pure fragment-shader distortion path is kept as a future benchmark
alternative; the mesh path is chosen first because it supports arbitrary
polynomial profiles without per-pixel polynomial work and keeps the render
thread allocation-free in steady state.

## Consequences

- The distortion math lives in `viewer` and is unit-tested.
- A 40x32 mesh per eye is uploaded on profile changes only.
- `RR-12` is covered by known-coefficient tests plus a checkerboard/grid
  visual check in the manual test plan.
