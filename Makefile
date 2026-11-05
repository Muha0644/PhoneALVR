# PhoneALVR common tasks. Run `make help` for the list.
SHELL := /bin/bash
.DEFAULT_GOAL := help

CARGO ?= cargo
GRADLE ?= ./gradlew
ABIS ?= arm64-v8a
PROFILE ?= release
APP_CORE := app/src/main/jniLibs/arm64-v8a/libphonealvr.so

.PHONY: help bootstrap fmt fmt-check lint test core core-if-missing build debug release install clean emulator-test

help: ## Show this help
	@grep -hE '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

bootstrap: ## Verify/install JDK, Android SDK/NDK, Rust targets, cargo-ndk
	./scripts/bootstrap.sh

fmt: ## Format Rust and Kotlin
	$(CARGO) fmt --all
	$(GRADLE) spotlessApply

fmt-check: ## Check formatting
	$(CARGO) fmt --all -- --check
	$(GRADLE) spotlessCheck

lint: ## Clippy + Android lint
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(GRADLE) lint

test: ## Host unit tests (core, viewer, render)
	$(CARGO) test --workspace

core: ## Build the prebuilt native core for $(ABIS) (PROFILE=release|debug)
	ABIS="$(ABIS)" PROFILE="$(PROFILE)" ./scripts/build-core.sh

core-if-missing:
	@test -f "$(APP_CORE)" || $(MAKE) core

build: core-if-missing ## Assemble a debug APK (uses prebuilt core; builds it if missing)
	$(GRADLE) :app:assembleDebug

debug: build ## Alias for build

release: core-if-missing ## Assemble a release APK (requires CI signing secrets for a real release)
	$(GRADLE) :app:assembleRelease

install: build ## Install the debug APK on the connected device
	adb install -r app/build/outputs/apk/debug/app-debug.apk

emulator-test: ## Run instrumented tests on the connected emulator (x86_64)
	$(GRADLE) :app:connectedDebugAndroidTest

clean: ## Remove build outputs
	$(CARGO) clean
	$(GRADLE) clean
	rm -rf dist app/src/main/jniLibs
