# sckit_capture

Lord Varys's system-audio sidecar. A standalone Swift CLI that uses
ScreenCaptureKit to capture system audio and writes raw PCM to stdout.

## Build

```bash
swift build -c release
```

The release binary lands at `.build/release/sckit_capture`. The Tauri build
expects it copied to `../binaries/sckit_capture-aarch64-apple-darwin` (or the
matching `x86_64` triple on Intel Macs):

```bash
mkdir -p ../binaries
cp .build/release/sckit_capture \
   ../binaries/sckit_capture-aarch64-apple-darwin
```

## Test standalone (no Tauri needed)

```bash
swift run -c release sckit_capture --format pcm-f32 --sample-rate 48000 --channels 2 > /tmp/out.raw
# stop with ctrl-C, then play back to verify:
ffplay -f f32le -ar 48000 -ac 2 /tmp/out.raw
```

The first time you run this, macOS will prompt for Screen Recording
permission. Once granted for the running terminal (or for Lord Varys when
launched as a child process), the dialog stops appearing.

## Probe mode

`--probe` starts SCKit briefly and exits — used by the Tauri Settings →
Permissions pane to surface the consent dialog without actually capturing.
Exit code 0 means SCKit was reachable; non-zero means the OS denied access
or SCKit threw.
