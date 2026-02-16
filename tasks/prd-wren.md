# PRD: Wren — Local Voice-to-Text App

## Introduction

Wren is a free, local-first voice-to-text dictation app for macOS. Users press a global hotkey, speak, and have their transcribed text auto-pasted into whatever app is focused. It replaces paid alternatives like SuperWhisper ($8/mo) by running OpenAI's Whisper model entirely on-device via whisper.cpp with Metal GPU acceleration. Built with Tauri v2 (Rust backend + React frontend), the app uses ~10-30MB memory vs Electron's 100-200MB.

## Goals

- Provide a fully local, zero-cost alternative to SuperWhisper and OpenWhispr
- Deliver sub-2-second transcription for typical dictation (5-30 seconds of speech) on Apple Silicon
- Work seamlessly across all macOS apps (text editors, browsers, chat apps, IDEs)
- Ship as a single `.dmg` with no external dependencies
- Support configurable global hotkey to avoid conflicts with Alfred/Raycast

## User Stories

### US-001: Create Tauri v2 project scaffold
**Description:** As a developer, I need the project scaffolded with Tauri v2 + React + TypeScript so that all subsequent features have a working foundation.

**Acceptance Criteria:**
- [ ] `npm create tauri-app` generates project with react-ts template
- [ ] `src-tauri/tauri.conf.json` configured: transparent overlay window (280x120), `macOSPrivateApi: true`, no decorations, `alwaysOnTop`, `visible: false`, `focus: false`
- [ ] `src-tauri/entitlements.plist` disables sandbox, enables audio-input
- [ ] `src-tauri/capabilities/default.json` grants core window + global-shortcut permissions
- [ ] Transparent body CSS applied (`background: transparent`, no scrollbars, no user-select)
- [ ] All Rust dependencies from PLAN.md added to `Cargo.toml` (whisper-rs with metal, cpal, arboard, enigo, tauri-plugin-global-shortcut, etc.)
- [ ] All frontend dependencies added to `package.json` (@tauri-apps/api, @tauri-apps/plugin-global-shortcut, tailwindcss)
- [ ] `npm run tauri dev` compiles and launches without errors (blank transparent window)

### US-002: Capture microphone audio
**Description:** As a user, I want the app to record my voice when I activate dictation so that my speech can be transcribed.

**Acceptance Criteria:**
- [ ] `AudioCapture` struct uses `cpal` to record from default input device
- [ ] Multi-channel audio converted to mono by averaging channels
- [ ] Supports both F32 and I16 sample formats
- [ ] `start_recording()` clears buffer and begins capturing
- [ ] `stop_recording()` returns resampled audio buffer (16kHz mono f32)
- [ ] Linear interpolation resampler converts device sample rate (44.1/48kHz) to 16kHz
- [ ] Recording verified: capture 3 seconds, confirm buffer length ~48000 samples (16000 * 3)

### US-003: Download and manage Whisper models
**Description:** As a user, I want the app to automatically download the speech recognition model on first run so I don't have to manually install anything.

**Acceptance Criteria:**
- [ ] Model catalog defines 5 models: tiny.en, base.en, small.en, medium.en, base.en-q8_0 with metadata (size, description)
- [ ] Models stored in `~/Library/Application Support/com.wren.app/models/`
- [ ] `download_model()` streams from HuggingFace with progress callback
- [ ] `model_exists()` checks if model file is already cached
- [ ] `model_path()` returns path to cached model file
- [ ] On first run, `base.en` model (148MB) downloads automatically
- [ ] Download progress emitted to frontend as `Downloading { progress: f32 }` state
- [ ] Overlay shows "Downloading model... XX%" during download
- [ ] Download resumes gracefully if app is restarted (partial files handled)

### US-004: Transcribe audio with Whisper
**Description:** As a user, I want my recorded speech converted to text locally so that my data never leaves my machine.

**Acceptance Criteria:**
- [ ] Dedicated transcription thread owns `WhisperContext` (not Send/Sync — never crosses thread boundaries)
- [ ] Channel-based API: `TranscriptionRequest` (LoadModel, Transcribe, Shutdown) / `TranscriptionResponse` (ModelLoaded, TranscriptionComplete)
- [ ] `spawn_transcription_thread()` returns (sender, receiver) pair
- [ ] Whisper params configured for dictation: Greedy sampling, 4 threads, English, no timestamps, suppress blanks
- [ ] Model loaded once at startup, reused for all transcriptions
- [ ] 5-second audio transcribes in ~0.3s on Apple Silicon with Metal
- [ ] Multi-segment output concatenated and trimmed
- [ ] Empty/silent audio returns empty string (no hallucinated text)

### US-005: Paste transcribed text into focused app
**Description:** As a user, I want transcribed text automatically pasted into whatever app I'm typing in so the experience is seamless.

**Acceptance Criteria:**
- [ ] `paste_text()` writes text to `NSPasteboard` via `arboard`
- [ ] 50ms delay after clipboard write for system readiness
- [ ] Simulates Cmd+V via `enigo` (CGEvent): Meta press → V click → Meta release
- [ ] Text appears in the previously focused app (TextEdit, VS Code, browser input, Slack, etc.)
- [ ] Text also remains on clipboard for manual Cmd+V paste
- [ ] `check_accessibility_permission()` calls `AXIsProcessTrusted()` on macOS
- [ ] If Accessibility permission not granted, app shows clear error message in overlay

### US-006: Wire up the dictation state machine
**Description:** As a developer, I need the core orchestration that connects hotkey → recording → transcription → paste into a seamless flow.

**Acceptance Criteria:**
- [ ] `DictationState` enum: Idle, Recording, Processing, Downloading, Error (serde-tagged)
- [ ] `SharedState` (Arc<Mutex<AppState>>) tracks current state, model path, selected model
- [ ] State transitions follow defined flow: Idle→Recording→Processing→Idle (or →Error→Idle)
- [ ] State changes emitted as `dictation-state` Tauri events to frontend
- [ ] Global hotkey (default: Option+Space) registered via `tauri-plugin-global-shortcut`
- [ ] Hotkey toggles: Idle→start recording, Recording→stop and transcribe
- [ ] Processing state ignores hotkey presses (no double-submit)
- [ ] Error state resets to Idle on next hotkey press
- [ ] Overlay window shown (without stealing focus) when recording/processing
- [ ] Overlay window hidden when returning to Idle
- [ ] On startup: check model exists → download if needed → load model → ready

### US-007: Build the floating overlay UI
**Description:** As a user, I want a minimal floating indicator so I know when the app is listening, processing, or encountering an error.

**Acceptance Criteria:**
- [ ] Dark frosted-glass pill (`bg-black/80 backdrop-blur-xl rounded-2xl`)
- [ ] Recording state: red pulsing dot (CSS `animate-ping`) + "Listening..." in red
- [ ] Processing state: blue spinning border + "Transcribing..." in blue
- [ ] Downloading state: green spinner + "Downloading model... XX%" in green
- [ ] Error state: yellow dot + error message text in yellow
- [ ] Idle state: component returns null (overlay hidden)
- [ ] `useDictationState` hook listens to `dictation-state` Tauri events
- [ ] Overlay does NOT steal focus from the target app (non-activating NSPanel behavior)
- [ ] Overlay visible on all Spaces/desktops (`NSWindowCollectionBehaviorCanJoinAllSpaces`)
- [ ] Overlay positioned near top of screen (y: 80)

### US-008: Make overlay window non-activating on macOS
**Description:** As a user, I need the overlay to appear without stealing keyboard focus from my current app, so text pastes into the right place.

**Acceptance Criteria:**
- [ ] `make_window_non_activating()` sets NSWindow collection behavior via `cocoa` crate
- [ ] Window has `NSWindowCollectionBehaviorCanJoinAllSpaces`, `Stationary`, `IgnoresCycle`
- [ ] Window level set to `NSFloatingWindowLevel`
- [ ] Verified: open TextEdit, trigger dictation, overlay appears, TextEdit retains focus, text pastes into TextEdit

### US-009: Handle permissions gracefully
**Description:** As a user, I want clear guidance when the app needs macOS permissions so I'm not confused by silent failures.

**Acceptance Criteria:**
- [ ] On startup, check Microphone permission status
- [ ] On startup, check Accessibility permission via `AXIsProcessTrusted()`
- [ ] If Microphone permission missing: overlay shows "Microphone access needed — check System Settings > Privacy > Microphone"
- [ ] If Accessibility permission missing: overlay shows "Accessibility access needed — check System Settings > Privacy > Accessibility"
- [ ] Permission errors use the Error state with clear, actionable messages
- [ ] App retries permission check on next hotkey press (user can grant permission and immediately retry)
- [ ] `NSMicrophoneUsageDescription` set in Info.plist for system permission dialog

### US-010: Make global hotkey configurable
**Description:** As a user, I want to change the dictation hotkey so it doesn't conflict with Alfred, Raycast, or other apps that use Option+Space.

**Acceptance Criteria:**
- [ ] Default hotkey is Option+Space
- [ ] Hotkey preference stored in `~/Library/Application Support/com.wren.app/config.json`
- [ ] Tauri command `set_hotkey` accepts new key combination, unregisters old, registers new
- [ ] Tauri command `get_hotkey` returns current hotkey string
- [ ] Frontend settings UI (simple modal or panel) shows current hotkey and allows changing it
- [ ] Invalid/already-registered hotkeys show error message
- [ ] Hotkey persists across app restarts

## Functional Requirements

- FR-1: The app must register a configurable system-wide global hotkey (default: Option+Space)
- FR-2: Pressing the hotkey while idle must start recording from the default microphone input
- FR-3: Pressing the hotkey while recording must stop recording and begin transcription
- FR-4: Audio must be captured as mono PCM and resampled to 16kHz for Whisper
- FR-5: Transcription must run locally via whisper.cpp (whisper-rs) with Metal acceleration
- FR-6: Transcribed text must be written to the system clipboard and auto-pasted via simulated Cmd+V
- FR-7: The floating overlay must show recording, processing, downloading, and error states
- FR-8: The overlay must not steal focus from the user's current application
- FR-9: On first launch, the app must auto-download the `base.en` model (148MB) from HuggingFace
- FR-10: Download progress must be displayed in the overlay UI
- FR-11: The app must check for Microphone and Accessibility permissions on startup
- FR-12: Missing permissions must produce clear, actionable error messages in the overlay
- FR-13: The app must run non-sandboxed (required for CGEvent keyboard simulation)
- FR-14: The global hotkey must be configurable and persisted to a config file

## Non-Goals (Out of Scope)

- No system tray icon (future enhancement)
- No model selection UI (auto-downloads base.en; model picker is a future feature)
- No Voice Activity Detection / auto-stop on silence
- No clipboard restoration (saving/restoring previous clipboard contents)
- No multi-language support (English-only `.en` models for MVP)
- No AI post-processing (grammar cleanup, formatting)
- No audio input device selection (uses system default)
- No Mac App Store distribution (requires non-sandboxed app)
- No Windows/Linux support in MVP

## Technical Considerations

- **whisper-rs compilation:** First `cargo build` takes 2-5 minutes as whisper.cpp compiles from source. Subsequent builds are cached.
- **WhisperState threading:** `WhisperState` is not `Send`/`Sync`. Must use a dedicated thread with `mpsc` channels — never hold across `.await` or pass between threads.
- **Memory usage:** 16kHz mono f32 audio is ~3.8MB/minute. 10 minutes = ~38MB. Not a concern for dictation.
- **macOS-specific APIs:** Uses `cocoa`/`objc` crates for NSWindow manipulation. `enigo` for CGEvent keyboard simulation. `AXIsProcessTrusted()` for permission checking.
- **Distribution:** Ship as `.dmg` (not App Store due to sandbox requirement).
- **Tech stack:** Tauri v2, Rust backend, React + TypeScript + Tailwind frontend, whisper-rs (Metal), cpal, arboard, enigo.

## Success Metrics

- Press hotkey → speak for 5 seconds → text appears in focused app in under 2 seconds total (transcription + paste)
- App memory usage stays under 50MB during idle, under 150MB during transcription
- base.en model produces accurate transcription for clear English speech in quiet environments
- Zero network calls after initial model download (fully offline)
- App launches in under 3 seconds (model already cached)

## Open Questions

- Should we preserve and restore the user's clipboard contents after pasting? (Deferred to future)
- What is the maximum recording duration before we should auto-stop? (No limit for MVP)
- Should the configurable hotkey UI be a separate settings window or inline in the overlay?

---

## Implementation Order

| Story | Phase | Focus |
|-------|-------|-------|
| US-001 | Phase 1 | Project scaffold |
| US-002 | Phase 2 | Audio capture |
| US-003 | Phase 3a | Model management |
| US-004 | Phase 3b | Transcription engine |
| US-005 | Phase 4 | Text insertion |
| US-006 | Phase 5 | Core orchestration |
| US-007 | Phase 6a | Overlay UI |
| US-008 | Phase 6b | Non-activating window |
| US-009 | Phase 5+ | Permission handling |
| US-010 | New | Configurable hotkey |

## Verification

End-to-end test flow:
1. `npm run tauri dev` — app launches, overlay hidden, no console errors
2. First run: base.en model downloads, overlay shows progress percentage
3. Press Option+Space: overlay shows red pulsing dot + "Listening..."
4. Speak into microphone for a few seconds
5. Press Option+Space again: overlay shows spinner + "Transcribing..."
6. Transcribed text appears in focused text field (TextEdit, VS Code, browser)
7. Text also on clipboard (Cmd+V to paste again)
8. Overlay hides after paste
9. Change hotkey in settings, verify new hotkey works
10. Revoke Accessibility permission, verify clear error message appears
