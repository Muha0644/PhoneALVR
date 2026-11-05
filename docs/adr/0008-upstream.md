# ADR 0008: Upstream strategy

- Status: accepted
- Date: 2026-11-05

## Decision

Keep PhoneALVR a standalone Cardboard/phone client for now. Contribute
capability improvements (for example a documented "phone display" backend or
resolution-bound fixes) upstream when they are generally useful, but do not
block v1 on an upstream merge.

## Consequences

- No local patches to ALVR are required by the normal build.
- Compatibility is documented per ALVR release and re-tested on each bump.
