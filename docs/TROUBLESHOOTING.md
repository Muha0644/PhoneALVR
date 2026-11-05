# Troubleshooting

## The phone never appears in ALVR

1. Confirm both devices are on the same LAN and the Wi-Fi does not use AP
   isolation (guest networks often do).
2. Press **Start waiting**; the waiting screen must show a LAN IP and ports
   9943/9944.
3. Add the phone manually in ALVR: **Connection → Add device → IP**.
4. Allow UDP/mDNS and TCP through the PC firewall. On Linux, check the
   `ufw`/`firewalld` rules for the ALVR process.
5. Wired: verify `adb devices` shows `device`, not `unauthorized`. See
   **Wired mode** below.

Remember: the phone listens. It will never connect to the PC by itself.

## "Port already in use"

Another app or a previous native session is still listening. Stop the app,
force-stop it, and check:

```bash
adb shell "ss -lntp | grep 994"
```

If the port is held by a stale process, reboot the phone or fix the offending
app. PhoneALVR reports this as a session error.

## Handshake / version mismatch

PhoneALVR targets ALVR **v20.14.1**. A different streamer protocol fails during
the server-initiated handshake with a visible error. Update ALVR or rebuild
PhoneALVR against the matching revision; do not expect a silent fallback.

## ALVR connects but the decoder fails (`Decoder dequeue error: ErrorUnknown`)

This is an ALVR client-core/MediaCodec failure, not a renderer failure. The
streamer connects and the decoder is configured, but the codec immediately
returns `ErrorUnknown`; ALVR restarts the stream in a loop.

Try, in this order:

1. **Lower the stream resolution.** Set ALVR → Video → Resolution to a lower
   scale/absolute size (for example 1024×1024 per eye) and retry. Very large
   side-by-side H.264 frames can exceed a phone decoder's capability.
2. **Disable adaptive bitrate and client post-processing.** ALVR issue #2843
   reports the same `ErrorUnknown` loop on some clients; disabling these made
   it stable for several users.
3. **Try HEVC.** Set ALVR → Video → Preferred codec to HEVC (the phone's HEVC
   decoder may handle the resolution better than its H.264 decoder).
4. **Turn off foveated encoding, 10-bit and HDR.** PhoneALVR advertises no
   foveation and 8-bit SDR; these must not be forced on the server.
5. **Try a software decoder** only as a diagnostic (expect low frame rate).

PhoneALVR passes the real encoded size (`2 × per-eye width × height`) to
MediaCodec in addition to ALVR's placeholders, and logs the H.264 profile and
CSD bytes. Capture:

```bash
adb logcat -c
adb logcat -s PhoneALVR PhoneALVR-Render AndroidRuntime
```

Look for `decoder config: ... profile ...`, `surfaceCreated`, `first decoded
frame`, and any `Decoder dequeue error`. If the error persists after a lower
resolution and HEVC, the next step is to replace ALVR's decoder release path
(`releaseOutputBufferAtTime`) and ImageReader sizing with a tested local
implementation or a maintained ALVR fork.

## Black screen

- Check `adb logcat -s PhoneALVR PhoneALVR-Render`.
- Confirm the viewer profile is valid (Settings → scan QR or reset to the
  built-in Cardboard v1 profile).
- Confirm the decoder was configured; look for `decoder config` and any
  `fatal decoder error` in logcat.
- A stream resolution larger than the panel is logged as an error; set ALVR
  Video → Resolution → Scale 1.0.
- If the surface was recreated, the render thread recreates EGL. If it still
  fails, leave and re-enter the VR activity.

## Distortion looks wrong / world is warped

- Scan the viewer's actual QR code. The built-in profile is only a fallback.
- Verify IPD/screen-to-lens in the profile; a wrongly generated QR code is
  common (see the Cardboard viewer-profile generator).
- Use a checkerboard/straight-grid scene to validate; a normal game can hide
  small radial errors.
- Reset to the built-in profile in Settings to isolate a bad custom profile.

## Latency, stutter, dropped frames

- Use 5 GHz/6 GHz Wi-Fi or USB wired mode.
- Lower the ALVR bitrate or resolution; check the encoder is not saturated.
- Disable battery optimisation for PhoneALVR (Android Settings → Apps →
  PhoneALVR → Battery → Unrestricted). The foreground service helps but cannot
  override an aggressive vendor killer.
- Capture evidence:

```bash
adb shell dumpsys gfxinfo dev.phonealvr.client
adb logcat -s PhoneALVR PhoneALVR-Render
# Perfetto: record sched + gfx + power for 30 s during a session
```

- The debug HUD (Settings → Debug HUD) shows FPS, frame time, latency,
  dropped frames and tracking rate.

## Screen sleeps or the session dies when backgrounded

- Allow notifications (Android 13+) so the foreground service notification is
  visible.
- Disable battery optimisation for the app.
- The VR activity keeps the screen on while it is in front; the service keeps
  the session while it is not.

## Wired mode

1. `adb devices` must list the phone as `device`.
2. ALVR → **Connection → Wired Client Type → Custom** must equal the app ID:
   `dev.phonealvr.client`
3. Optional: enable **Wired Client Autolaunch**.
4. Deprecated fallback:

```bash
adb forward tcp:9943 tcp:9943
adb forward tcp:9944 tcp:9944
```

Then select TCP + `127.0.0.1` in ALVR.

## Exporting diagnostics

Use **Diagnostics** on the home screen. It writes a JSON report (device,
settings with the crash endpoint removed, native session/stats) and opens the
share sheet. Attach it to a bug report.

## Log tags

- `PhoneALVR` — native session, ALVR events, renderer
- `PhoneALVR-Render` — render thread failures
- ALVR core logs are emitted through the same native logging backend.
