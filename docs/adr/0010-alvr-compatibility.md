# ADR 0010: ALVR compatibility policy

- Status: accepted
- Date: 2026-11-05

## Decision

Target exactly ALVR streamer **v20.14.1** for v1. On a mismatch, fail fast in
the server-initiated handshake with a clear error in the UI and logs; never
silently hang. There is no compatibility shim for other protocol versions.

## Consequences

- `docs/COMPATIBILITY.md` records the tested client and streamer revisions.
- A protocol bump is a deliberate change with a new ADR, new CI artifacts and
  a new manual test pass.
