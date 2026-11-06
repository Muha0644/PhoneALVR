#!/usr/bin/env bash
# Build phonealvr-native (ALVR client core + renderer + JNI) per Android ABI.
#
# The normal Gradle build consumes the resulting jniLibs; it never invokes
# Cargo. Rebuild this only when the Rust/ALVR code or the pin changes.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ABIS="${ABIS:-arm64-v8a}"
PROFILE="${PROFILE:-release}"
OUT="${OUT:-app/src/main/jniLibs}"

NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [[ -z "$NDK" ]]; then
  SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
  if [[ -f local.properties ]]; then
    SDK="$(sed -n 's/^sdk.dir=//p' local.properties | head -1)"
  fi
  if [[ -d "$SDK/ndk" ]]; then
    NDK="$(find "$SDK/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)"
  fi
fi
[[ -n "$NDK" && -d "$NDK" ]] || {
  echo "error: Android NDK not found. Set ANDROID_NDK_HOME." >&2
  exit 1
}
export ANDROID_NDK_HOME="$NDK"
command -v cargo-ndk >/dev/null 2>&1 || {
  echo "error: cargo-ndk not found. Run scripts/bootstrap.sh." >&2
  exit 1
}

PROFILE_ARGS=()
if [[ "$PROFILE" == "release" ]]; then
  PROFILE_ARGS=(--release)
fi

mkdir -p "$OUT"
echo "==> NDK: $NDK"
echo "==> ABIs: $ABIS"
echo "==> Profile: $PROFILE"
for abi in $ABIS; do
  echo "==> Building $abi"
  cargo ndk -p 26 -t "$abi" -o "$OUT" build -p phonealvr-native "${PROFILE_ARGS[@]}"
done

echo
echo "==> Native artifacts in $OUT"
find "$OUT" -name '*.so' -print0 | sort -z | xargs -0 -r ls -lh
