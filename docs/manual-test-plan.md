# Manual test plan

Record the date, phone, Android build, ALVR version, PC, GPU and network for
every run. Attach logs/traces. Do not mark a row "pass" without evidence.

## A. Wireless first frame

- [ ] Start ALVR on the PC; phone and PC on the same 5 GHz/6 GHz LAN.
- [ ] Press **Start waiting**; phone appears in the ALVR dashboard.
- [ ] Trust the device; start a SteamVR app.
- [ ] First rendered frame appears within 5 s of the stream starting (PR-6).
- [ ] Stereo image is visible in both eyes with no black half.
- [ ] Checkerboard/grid scene: straight lines look straight through the lenses.
- [ ] Recenter (tap/volume-down) puts the current view forward.
- [ ] No view-param spam in logs; SteamVR does not rebuild distortion meshes
      continuously.
- [ ] Stop from the notification; app returns to idle without a crash.
- [ ] Start again; streamer reconnects without restarting the app.

## B. Wired / USB

- [ ] `adb devices` shows `device`.
- [ ] ALVR wired client type is Custom + `dev.phonealvr.client`.
- [ ] Wired connection reaches streaming.
- [ ] Unplug/replug USB; the listener stays up and the streamer reconnects
      within 5 s of the link returning.
- [ ] Deprecated `adb forward` fallback also works.

## C. Lifecycle and reliability

- [ ] 100× start/stop cycles (UI and notification) without crash/leak.
- [ ] Screen off/on during streaming: session survives or cleanly restarts.
- [ ] Surface recreation (rotate/enter picture-in-picture if allowed) does not
      leak EGL objects or crash.
- [ ] App backgrounded/foregrounded while streaming; foreground notification
      remains.
- [ ] Wi-Fi off/on; reconnect within 5 s.
- [ ] Server disappears mid-frame; no use-after-free or ANR.
- [ ] Deny camera permission; QR scan is unavailable but the app works.
- [ ] Deny notification permission; service still functions as far as the OS
      allows.
- [ ] 60-minute soak: no crash/ANR; memory growth < 5%.

## D. Viewer profiles

- [ ] Built-in Cardboard v1 profile renders.
- [ ] Scan a modern QR (protobuf `?p=`) and a JSON profile; both apply.
- [ ] Invalid QR is rejected with a message and the old profile is retained.
- [ ] IPD change moves the eyes correctly and sends one `ViewsConfig` update,
      not a stream.

## E. Performance

- [ ] Render-thread frame time < 11 ms at 72 Hz and < 8 ms at 90 Hz.
- [ ] Dropped frames < 1% over 10 minutes excluding network/encoder stalls.
- [ ] App-added motion-to-photon latency ≤ 1 vsync beyond the core baseline;
      record ALVR statistics and a high-speed-camera measurement if available.
- [ ] Release APK size < 40 MB per ABI (or record the actual size).
- [ ] Perfetto trace + `dumpsys gfxinfo` attached.

## F. Emulator (x86_64)

- [ ] App launches and idle screen shows network/ports.
- [ ] Permission-denial paths behave.
- [ ] Incoming-connection failure path shows an error (mock/no server).
- [ ] Lifecycle cycling with the mock transport passes.
- [ ] Do not use adb telnet sensor injection as the only tracking/rendering
      correctness test.
