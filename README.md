# Lord Varys

Ambient AI memory for work — a tray-only macOS app that silently records virtual meetings (Google Meet, Zoom, MS Teams) without joining as a bot.

**Milestone 1 scope:** detect a meeting → capture system + microphone audio → save a single mixed WAV. No transcription, no summaries, no chat. Those land in M2+.

## Stack

- **Tauri 2** (Rust core + React/TypeScript settings UI)
- **macOS 13+** (ScreenCaptureKit for system audio)
- **`cpal`** for microphone capture
- **Swift sidecar binary** (`sckit_capture`) for system audio capture
- **Chrome extension** for browser-meeting detection
- **`NSWorkspace`** poller for native Zoom/Teams detection

## Build

```bash
# Frontend deps
pnpm install

# One-time: create a self-signed code-signing cert in your Keychain so
# every rebuild keeps the same codesign identity. Without this, macOS
# treats every rebuild as a brand-new app and TCC grants (Microphone,
# Screen Recording) don't persist.
./scripts/setup-signing.sh

# Build the Swift sidecar (required before tauri dev/build)
cd src-tauri/swift-helper && swift build -c release && cd ../..
mkdir -p src-tauri/binaries
cp src-tauri/swift-helper/.build/release/sckit_capture \
   src-tauri/binaries/sckit_capture-aarch64-apple-darwin

# Run in dev mode
pnpm tauri:dev

# Production build (signs with the self-signed cert via tauri.conf.json)
pnpm tauri:build
```

## Permissions required

- **Microphone** — to capture the user's voice
- **Screen Recording** — required by ScreenCaptureKit for system audio (the OS prompts when SCKit first runs; there's no programmatic API)

## Chrome extension

Install via `chrome://extensions` → Load unpacked → select `chrome-extension/`. The extension watches Meet/Zoom/Teams DOM and POSTs meeting events to the local Tauri app on `127.0.0.1:43117`.

## Repo layout

```
src/                    React frontend (Settings window)
src-tauri/
  src/                  Rust backend
    audio_mic.rs        cpal mic capture, streaming
    audio_system.rs     sckit_capture sidecar bridge
    audio_mixer.rs      mix mic + system → 16 kHz mono WAV
    recording.rs        RecordingSession orchestration
    detection/          meeting detection (chrome_ext + native_apps)
    onboarding.rs       mic + screen recording permission flow
    settings.rs         SQLite KV preferences
    state.rs            AppState
    tray.rs             menu bar UI
    lib.rs              Tauri setup
  swift-helper/         Swift sidecar (Package.swift + Sources/)
chrome-extension/       MV3 companion extension
```
