# Building PhoneALVR

PhoneALVR has two layers:

1. **Rust native core** (`native/` + `core/` + `viewer/` + `render/`) — built
   once per Android ABI by `scripts/build-core.sh`.
2. **Android app** (`app/`) — Kotlin/Compose; consumes the prebuilt
   `libphonealvr.so` from `app/src/main/jniLibs/`.

The normal Gradle build does **not** invoke Cargo.

## Pinned toolchain

| Tool | Version | Where |
| --- | --- | --- |
| JDK | 17+ | `JAVA_HOME` |
| Gradle | 8.10.2 | `gradle/wrapper/gradle-wrapper.properties` |
| Android Gradle Plugin | 8.7.3 | `gradle/libs.versions.toml` |
| Kotlin | 2.0.21 | `gradle/libs.versions.toml` |
| compile/target SDK | 35 | `app/build.gradle.kts` |
| min SDK | 26 | `app/build.gradle.kts` |
| Android NDK | r27c (27.x) | `ANDROID_NDK_HOME` or `$ANDROID_HOME/ndk` |
| Rust | 1.88.0 | `rust-toolchain.toml` |
| cargo-ndk | 3.5.4 | installed by `scripts/bootstrap.sh` |
| ALVR client core | v20.14.1 | `Cargo.toml` / `Cargo.lock` |

## One-command bootstrap

```bash
./scripts/bootstrap.sh
```

It verifies Java 17+, the Android SDK, the NDK, Rust targets and
`cargo-ndk`, fetches the pinned Cargo graph, and prints the next commands.
It does not install a JDK or NDK for you; install those through your platform
package manager or Android Studio / `sdkmanager`.

## Build the native core

```bash
# arm64 phone (default)
make core

# emulator + phone
ABIS="arm64-v8a x86_64" PROFILE=debug ./scripts/build-core.sh

# optimized release core
ABIS="arm64-v8a" PROFILE=release ./scripts/build-core.sh
```

Artifacts land in `app/src/main/jniLibs/<abi>/libphonealvr.so`. This directory
is git-ignored. CI builds both `arm64-v8a` and `x86_64` and caches the result.

The first build compiles ALVR's client core and its transitive Rust
dependencies; expect several minutes and ~10 GB of disk.

## Build the app

```bash
make build                 # debug APK (arm64 + x86_64)
make release               # release APK (arm64 only)
make install               # adb install the debug APK
```

Outputs:

- `app/build/outputs/apk/debug/app-debug.apk`
- `app/build/outputs/apk/release/app-release.apk`

### ABI policy

- Release: `arm64-v8a` only.
- Debug: `arm64-v8a` and `x86_64` (emulator).

The Gradle filters are in `app/build.gradle.kts`; the native artifact must
exist for every enabled ABI.

### Release signing

No keystore or password exists in this repository. A release build reads these
environment variables and creates the signing config only when
`PHONEALVR_KEYSTORE` is set:

```bash
export PHONEALVR_KEYSTORE=/secure/path/release.jks
export PHONEALVR_KEYSTORE_PASSWORD='…'
export PHONEALVR_KEY_ALIAS='…'
export PHONEALVR_KEY_PASSWORD='…'
./gradlew :app:assembleRelease
```

Without them the release APK is produced unsigned and is not distributable.
CI supplies the secrets; PR builds use the debug signing config.

## Tests

```bash
make test                  # cargo test --workspace (host)
make lint                  # clippy + Android lint
./gradlew :app:testDebugUnitTest
./gradlew :app:connectedDebugAndroidTest   # needs an emulator/device
```

The host suite covers the session state machine, 100 start/stop cycles,
view-param deduplication, tracking prediction/recentering, settings validation,
statistics, viewer JSON/protobuf parsing, distortion math, mesh topology and
projection math.

See `docs/manual-test-plan.md` for the real-device ALVR checklist and
`docs/COMPATIBILITY.md` for the current test status.

## Emulator

Build the x86_64 core and run an Android 30+ AVD with OpenGL ES 3:

```bash
ABIS="x86_64" PROFILE=debug ./scripts/build-core.sh
./gradlew :app:assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

For ALVR networking through the emulator, forward the listener ports:

```bash
adb forward tcp:9943 tcp:9943
adb forward tcp:9944 tcp:9944
```

Emulator sensor injection is a convenience only. It is not the primary
tracking/rendering correctness test (see TEST-10).

## Rebuilding after changing the ALVR pin

1. Update the tag/revision in `Cargo.toml` and the workspace dependencies.
2. Update `rust-toolchain.toml` if ALVR's MSRV changed.
3. Run `cargo update` and commit `Cargo.lock`.
4. Rebuild both ABIs and run the host tests and the manual test plan.
5. Update `docs/COMPATIBILITY.md` and the relevant ADR.

## `make` targets

```
bootstrap   verify/install toolchains
fmt         cargo fmt + spotlessApply
fmt-check   cargo fmt --check + spotlessCheck
lint        clippy + Android lint
test        host unit tests
core        build the native core
build       debug APK
release     release APK
install     install the debug APK
clean       remove build outputs
```
