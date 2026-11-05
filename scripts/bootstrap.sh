#!/usr/bin/env bash
# Verify (and where safe, install) the toolchains required by PhoneALVR.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() { echo "error: $*" >&2; exit 1; }
note() { echo "==> $*"; }

note "Checking Java"
if [[ -n "${JAVA_HOME:-}" && -x "$JAVA_HOME/bin/java" ]]; then
  JAVA="$JAVA_HOME/bin/java"
elif command -v java >/dev/null 2>&1; then
  JAVA="java"
else
  fail "JDK 17+ not found. Set JAVA_HOME or install a JDK 17."
fi
JAVA_VERSION="$("$JAVA" -version 2>&1 | awk -F'"' '/version/ {print $2; exit}')"
JAVA_MAJOR="${JAVA_VERSION%%.*}"
[[ "$JAVA_MAJOR" =~ ^[0-9]+$ ]] || fail "could not parse Java version: $JAVA_VERSION"
[[ "$JAVA_MAJOR" -ge 17 ]] || fail "JDK 17+ required (found $JAVA_VERSION)"
note "Java $JAVA_VERSION"

note "Checking Android SDK"
if [[ -z "${ANDROID_HOME:-}" && -f local.properties ]]; then
  ANDROID_HOME="$(sed -n 's/^sdk.dir=//p' local.properties | head -1)"
fi
ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
[[ -n "$ANDROID_HOME" && -d "$ANDROID_HOME" ]] || fail \
  "Android SDK not found. Set ANDROID_HOME or create local.properties with sdk.dir=..."
export ANDROID_HOME
note "Android SDK: $ANDROID_HOME"

note "Checking Android NDK"
NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [[ -z "$NDK" && -d "$ANDROID_HOME/ndk" ]]; then
  NDK="$(find "$ANDROID_HOME/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)"
fi
if [[ -z "$NDK" || ! -d "$NDK" ]]; then
  echo "warning: Android NDK not found; only prebuilt-core Android builds will work."
  echo "         Install it with: sdkmanager 'ndk;27.0.12077973'"
else
  export ANDROID_NDK_HOME="$NDK"
  note "Android NDK: $NDK"
fi

note "Checking Rust toolchain and Android targets"
command -v cargo >/dev/null 2>&1 || fail "Rust/cargo not found; install from https://rustup.rs"
rustup target add aarch64-linux-android x86_64-linux-android

note "Checking cargo-ndk"
if ! command -v cargo-ndk >/dev/null 2>&1; then
  note "Installing cargo-ndk 3.5.4"
  cargo install cargo-ndk --version 3.5.4 --locked
fi
cargo ndk --version

note "Fetching pinned Rust dependencies"
cargo fetch

cat <<'MSG'

Bootstrap complete.

Next:
  make core         # build the native core for arm64-v8a (needs the NDK)
  make build        # assemble the debug APK
  make test         # host unit tests
MSG
