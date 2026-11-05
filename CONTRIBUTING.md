# Contributing

## Before you start

- Read `PROMPT.md`, `docs/ARCHITECTURE.md` and the ADRs.
- For an OPEN decision or a large irreversible change, add an ADR and discuss
  it before implementing.
- Never commit secrets, keystores, signing passwords, tokens or hardcoded
  webhook endpoints.

## Coding standards

- Rust: `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings`.
  Keep `core`, `viewer` and `render` free of Android imports.
- Kotlin: Google style, 4-space indent, formatted with Spotless/ktfmt
  (`./gradlew spotlessApply`). Prefer small Compose functions and stateless
  previews.
- Native: no protocol logic in JNI; no per-frame allocation/lock on the render
  thread; document every `unsafe` block.
- XML/JSON: 4 spaces; stable attribute order.

## Tests

Every behaviour change needs a test at the cheapest level:

- policy/math → host unit test;
- JNI/session lifecycle → `native::Runtime` test;
- status/settings serialization → Kotlin unit test;
- screens → Compose UI test;
- real device/pairing/soak → update `docs/manual-test-plan.md` and record
  evidence.

Run before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./gradlew :app:testDebugUnitTest :app:lintDebug
```

## Commit style

Conventional Commits, imperative mood, one logical change:

```
feat(core): deduplicate view parameters
fix(render): recreate EGL image after surface loss
docs(adr): accept Rust-native renderer
test(viewer): cover protobuf QR payloads
```

Reference the requirement ID (for example `FR-14`, `RR-5`) in the body when it
helps. Include logs/traces/benchmarks for performance/reliability work.

## Pull requests

- Keep the tree buildable; no half-finished vertical slices.
- Update documentation and `CHANGELOG.md` with the change.
- State what was tested and on which device/emulator/ALVR version.
- Do not mix formatting-only changes with behaviour changes.
