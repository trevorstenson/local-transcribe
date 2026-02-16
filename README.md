# Wren

Wren is a macOS dictation app that runs locally.

Press a hotkey, talk, press again, and your text is pasted into the app you were using.
Transcription runs on-device with Whisper (`whisper-rs` + Metal).

## What It Does

- Global hotkey to start/stop recording (default: `Option+Space`)
- Floating status overlay while recording/transcribing/downloading
- Auto-download and switch between Whisper models
- Smart Paste mode: if a text field is focused, Wren pastes immediately; otherwise it copies text to your clipboard
- Menu bar settings for hotkey, model, and Smart Paste

## Installing a Release

Download the latest `.dmg` from the [Releases](../../releases) page. Open the DMG and drag Wren to Applications.

macOS may block the app because it is not yet notarized. To open it, run:

```bash
xattr -cr /Applications/Wren.app
```

Then open Wren normally.

## Requirements

- macOS 10.15 or later
- Node.js 18+
- Rust (stable) + Cargo
- Xcode Command Line Tools
- `cmake` (needed to build `whisper-rs` / `whisper.cpp`)

## Run From Source

```bash
npm install
npm run tauri dev
```

Notes:
- The first build can take a while because Whisper is compiled from source.
- On first launch, Wren downloads `base.en` (~148 MB) from Hugging Face.

## Using Wren

1. Launch the app.
2. Grant macOS permissions when prompted (Microphone and Accessibility).
3. Press `Option+Space` to start dictation.
4. Press `Option+Space` again to stop.
5. Wren transcribes and pastes (or copies to clipboard when Smart Paste blocks auto-paste).

Open the menu bar icon and click `Settings...` to change hotkey, model, and Smart Paste.

If the menu bar icon is hidden on smaller displays, use:
- `Command+Option+,` to open Settings
- `Command+Option+H` to open History

## Models

English models currently available:

- `tiny.en` (~78 MB): fastest, least accurate
- `base.en` (~148 MB): default, balanced
- `small.en` (~488 MB): more accurate, slower
- `medium.en` (~1.5 GB): most accurate, slowest
- `base.en-q8_0` (~82 MB): quantized base model, good speed/quality tradeoff

## Paths

- Config: `~/Library/Application Support/com.wren.app/config.json`
- Models: `~/Library/Application Support/com.wren.app/models/`

## Build A Release

```bash
npm run tauri build
```

Artifacts are written to `src-tauri/target/release/bundle/`.

### Code Signing & Notarization (Optional)

To distribute builds that open without the `xattr` workaround, you need an [Apple Developer account](https://developer.apple.com/programs/) ($99/year) with a **Developer ID Application** certificate.

Export these environment variables before building:

```bash
# Code signing
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_CERTIFICATE="<base64-encoded .p12 certificate>"
export APPLE_CERTIFICATE_PASSWORD="<certificate password>"

# Notarization
export APPLE_ID="you@example.com"
export APPLE_PASSWORD="<app-specific password>"
export APPLE_TEAM_ID="XXXXXXXXXX"
```

Then build with the helper script:

```bash
./scripts/build-release.sh
```

Tauri automatically signs and notarizes the app when these variables are set.

## Releases

Versioned builds and release notes are published on the repository's **Releases** page.

## Scope

- macOS-focused
- English Whisper models (`*.en`) only
- No cloud transcription

## License

No `LICENSE` file is currently included in this repo.
Add one before public open-source distribution.
