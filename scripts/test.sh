#!/usr/bin/env bash
# Host checks that do not require a device or an Android emulator.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> clippy"
cargo clippy --workspace --all-targets -- -D warnings

echo "==> cargo test --workspace"
cargo test --workspace

if [[ -x ./gradlew && -n "${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}" ]]; then
  echo "==> Gradle unit tests + lint"
  ./gradlew :app:testDebugUnitTest :app:lintDebug --console=plain
else
  echo "==> Skipping Gradle (no ./gradlew or ANDROID_HOME)"
fi
