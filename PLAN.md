# Wren: Local Voice-to-Text App

## Context

SuperWhisper ($8/mo) and OpenWhispr (freemium) are dictation apps that let you press a hotkey, speak, and have text auto-pasted into whatever app is focused. Both use OpenAI's Whisper model for transcription. SuperWhisper is a native Swift app using whisper.cpp; OpenWhispr is Electron + React. We're building a completely free, local-first alternative using **Tauri v2 + Rust + React** with **whisper.cpp** (via `whisper-rs` Rust bindings).

---

## How These Apps Work Internally

```
1. User presses global hotkey (Option+Space)
2. App captures microphone audio (16kHz mono PCM)
3. Floating overlay shows recording state
4. User presses hotkey again to stop
5. Audio sent to local Whisper model for transcription
6. Transcribed text written to clipboard
7. App simulates Cmd+V to paste into focused app
8. Overlay hides
```

### Key Components

**Speech-to-Text Engine:** Both apps use OpenAI's Whisper model. SuperWhisper uses whisper.cpp directly (confirmed on HN) with CoreML acceleration. OpenWhispr supports multiple engines including whisper.cpp and sherpa-onnx (NVIDIA Parakeet). Models range from `tiny` (~39M params, 78MB) to `large-v3` (~1.5B params). The `base.en` model (148MB) is the sweet spot for dictation -- fast enough to feel instant, accurate enough for natural speech.

**Audio Capture:** On macOS, apps use AVAudioEngine (Swift) or CoreAudio (via cpal in Rust). Audio must be converted to 16kHz mono PCM float32 -- Whisper's expected input format. Device sample rates are typically 44.1kHz or 48kHz, requiring resampling.

**Global Hotkeys:** macOS apps register system-wide hotkeys via CGEventTap (low-level, requires Accessibility permission) or the Carbon RegisterEventHotKey API (legacy but functional). Swift libraries like `HotKey` and `KeyboardShortcuts` wrap these APIs. Tauri provides `tauri-plugin-global-shortcut`.

**Text Insertion:** This is the trickiest part. All these apps use the same two-step pattern:
1. Write text to `NSPasteboard` (clipboard)
2. Simulate Cmd+V via `CGEvent` to paste into the focused app

This requires **Accessibility permission** (System Settings > Privacy & Security > Accessibility) and the app must be **non-sandboxed** (can't use Mac App Store).

**Floating Overlay:** Native apps use `NSPanel` (a lightweight `NSWindow` subclass) with `level = .floating` and non-activating behavior so it doesn't steal focus. Tauri/Electron apps use `alwaysOnTop: true` + `transparent: true` + `decorations: false`. Critical: the overlay must NOT activate/focus the app -- the user needs keyboard focus to stay in their target app.

**macOS Permissions Required:**
- **Microphone** (`NSMicrophoneUsageDescription` in Info.plist)
- **Accessibility** (for CGEvent keyboard simulation -- requires non-sandboxed app)

---

## Architecture

### Tech Stack
| Component | Technology | Why |
|-----------|-----------|-----|
| App framework | Tauri v2 | 10-30MB memory (vs Electron's 100-200MB), Rust backend, React frontend |
| STT engine | whisper-rs (whisper.cpp bindings) | Compiles from source, Metal GPU acceleration on Apple Silicon |
| Audio capture | cpal | Cross-platform, uses CoreAudio on macOS, zero-config |
| Keyboard simulation | enigo | Cross-platform CGEvent wrapper |
| Clipboard | arboard | Cross-platform clipboard access |
| Frontend | React + TypeScript + Tailwind | Lightweight overlay UI |

### Project Structure
```
local-transcribe/
├── src-tauri/
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json               # Window config, permissions, bundle settings
│   ├── capabilities/
│   │   └── default.json              # Tauri v2 capability permissions
│   ├── entitlements.plist             # macOS: no sandbox, audio-input
│   └── src/
│       ├── lib.rs                     # Tauri setup, plugin registration, commands, shortcut handler
│       ├── main.rs                    # Entry point (calls lib::run)
│       ├── state.rs                   # DictationState enum, SharedState type
│       ├── audio/
│       │   ├── mod.rs
│       │   ├── capture.rs             # cpal mic recording, mono conversion, buffering
│       │   └── resampler.rs           # Linear interpolation resample to 16kHz
│       ├── transcription/
│       │   ├── mod.rs
│       │   ├── whisper.rs             # WhisperContext, dedicated thread, channel-based API
│       │   └── model_manager.rs       # Model catalog, download from HuggingFace, cache
│       └── input/
│           ├── mod.rs
│           └── paste.rs               # Clipboard write + Cmd+V simulation
├── src/
│   ├── main.tsx                       # React entry point
│   ├── App.tsx                        # Root component
│   ├── types.ts                       # TypeScript types matching Rust event payloads
│   ├── hooks/
│   │   └── useDictationState.ts       # Listen to Tauri "dictation-state" events
│   ├── components/
│   │   ├── Overlay.tsx                # Main overlay: recording/processing/error states
│   │   ├── PulseAnimation.tsx         # Red pulsing dot for recording state
│   │   └── StatusText.tsx             # Status text display
│   └── styles/
│       └── globals.css                # Tailwind imports, transparent body
├── index.html
├── package.json
├── tsconfig.json
├── tailwind.config.js
├── postcss.config.js
└── vite.config.ts
```

### Key Rust Dependencies
```toml
[dependencies]
tauri = { version = "2.10", features = ["macos-private-api"] }
tauri-plugin-global-shortcut = "2.3"
whisper-rs = { version = "0.15", features = ["metal"] }
cpal = "0.17"
arboard = "3.6"
enigo = "0.6"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "sync", "time"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
log = "0.4"
env_logger = "0.11"
reqwest = { version = "0.12", features = ["stream", "rustls-tls"] }
futures-util = "0.3"
dirs = "6"
parking_lot = "0.12"

[target.'cfg(target_os = "macos")'.dependencies]
cocoa = "0.26"
objc = "0.2"

[build-dependencies]
tauri-build = { version = "2.0", features = [] }
```

### Key Frontend Dependencies
```json
{
  "dependencies": {
    "@tauri-apps/api": "^2.2.0",
    "@tauri-apps/plugin-global-shortcut": "^2.2.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.2.0",
    "@vitejs/plugin-react": "^4.3.0",
    "tailwindcss": "^3.4.0",
    "typescript": "^5.6.0",
    "vite": "^6.0.0"
  }
}
```

---

## Detailed Implementation Plan

### Phase 1: Project Scaffold

**1. Create Tauri v2 project**
```bash
cd ~/Development/local-transcribe
npm create tauri-app@latest . -- --template react-ts
```

**2. Configure `src-tauri/tauri.conf.json`**
```json
{
  "productName": "Wren",
  "identifier": "com.wren.app",
  "app": {
    "macOSPrivateApi": true,
    "windows": [
      {
        "label": "overlay",
        "title": "Wren",
        "width": 280,
        "height": 120,
        "y": 80,
        "resizable": false,
        "decorations": false,
        "transparent": true,
        "alwaysOnTop": true,
        "skipTaskbar": true,
        "visible": false,
        "focus": false,
        "shadow": false
      }
    ]
  },
  "bundle": {
    "macOS": {
      "entitlements": "./entitlements.plist",
      "infoPlist": {
        "NSMicrophoneUsageDescription": "Wren needs microphone access to record your voice for transcription."
      }
    }
  }
}
```

**3. Create `src-tauri/entitlements.plist`**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.app-sandbox</key>
    <false/>
    <key>com.apple.security.device.audio-input</key>
    <true/>
</dict>
</plist>
```

**4. Create `src-tauri/capabilities/default.json`**
```json
{
  "identifier": "default",
  "windows": ["overlay"],
  "permissions": [
    "core:default",
    "core:window:allow-show",
    "core:window:allow-hide",
    "core:window:allow-set-focus",
    "core:window:allow-set-position",
    "global-shortcut:allow-register",
    "global-shortcut:allow-unregister",
    "global-shortcut:allow-is-registered"
  ]
}
```

**5. Set up transparent body CSS** (`src/styles/globals.css`)
```css
@tailwind base;
@tailwind components;
@tailwind utilities;

html, body, #root {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
  background: transparent;
  overflow: hidden;
  -webkit-user-select: none;
  user-select: none;
}
```

---

### Phase 2: Audio Capture

**6. `src-tauri/src/audio/capture.rs`**

Uses `cpal` to record from the default input device. Captures audio into a `Vec<f32>` buffer, converting multi-channel to mono by averaging.

```rust
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::sync::Arc;
use parking_lot::Mutex;

pub struct AudioCapture {
    stream: Option<Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    device_sample_rate: u32,
}

impl AudioCapture {
    pub fn new() -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device found"))?;
        let config = device.default_input_config()?;

        Ok(Self {
            stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            device_sample_rate: config.sample_rate().0,
        })
    }

    pub fn start_recording(&mut self) -> anyhow::Result<()> {
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device found"))?;
        let config = device.default_input_config()?;
        let channels = config.channels() as usize;
        self.device_sample_rate = config.sample_rate().0;

        let buffer = self.buffer.clone();
        buffer.lock().clear();

        let stream_config: StreamConfig = config.clone().into();
        let stream = match config.sample_format() {
            SampleFormat::F32 => {
                device.build_input_stream(
                    &stream_config,
                    move |data: &[f32], _| {
                        let mono: Vec<f32> = data.chunks(channels)
                            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                            .collect();
                        buffer.lock().extend_from_slice(&mono);
                    },
                    |err| log::error!("Audio stream error: {}", err),
                    None,
                )?
            }
            SampleFormat::I16 => {
                device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _| {
                        let mono: Vec<f32> = data.chunks(channels)
                            .map(|frame| {
                                frame.iter()
                                    .map(|&s| s as f32 / i16::MAX as f32)
                                    .sum::<f32>() / channels as f32
                            })
                            .collect();
                        buffer.lock().extend_from_slice(&mono);
                    },
                    |err| log::error!("Audio stream error: {}", err),
                    None,
                )?
            }
            _ => return Err(anyhow::anyhow!("Unsupported sample format")),
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop_recording(&mut self) -> Vec<f32> {
        self.stream.take(); // Drop stream to stop recording
        let raw_buffer = std::mem::take(&mut *self.buffer.lock());
        super::resampler::resample(&raw_buffer, self.device_sample_rate, 16000)
    }
}
```

**7. `src-tauri/src/audio/resampler.rs`**

Linear interpolation resampler. Sufficient quality for speech (not music production).

```rust
pub fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (input.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 * ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;

        let sample = if idx + 1 < input.len() {
            input[idx] as f64 * (1.0 - frac) + input[idx + 1] as f64 * frac
        } else {
            input[idx.min(input.len() - 1)] as f64
        };
        output.push(sample as f32);
    }

    output
}
```

---

### Phase 3: Transcription Engine

**8. `src-tauri/src/transcription/model_manager.rs`**

Models are downloaded from HuggingFace and cached in the app's data directory.

```rust
use std::path::PathBuf;

const HF_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub struct ModelInfo {
    pub name: &'static str,
    pub filename: &'static str,
    pub size_mb: u32,
    pub description: &'static str,
}

pub const AVAILABLE_MODELS: &[ModelInfo] = &[
    ModelInfo { name: "tiny.en",      filename: "ggml-tiny.en.bin",      size_mb: 78,   description: "Fastest, lowest quality" },
    ModelInfo { name: "base.en",      filename: "ggml-base.en.bin",      size_mb: 148,  description: "Good balance (recommended)" },
    ModelInfo { name: "small.en",     filename: "ggml-small.en.bin",     size_mb: 488,  description: "Better quality, slower" },
    ModelInfo { name: "medium.en",    filename: "ggml-medium.en.bin",    size_mb: 1530, description: "High quality, much slower" },
    ModelInfo { name: "base.en-q8_0", filename: "ggml-base.en-q8_0.bin", size_mb: 82,   description: "Quantized base, fast" },
];

pub fn models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.wren.app")
        .join("models")
}

pub fn model_exists(model_name: &str) -> bool {
    AVAILABLE_MODELS.iter()
        .find(|m| m.name == model_name)
        .map(|info| models_dir().join(info.filename).exists())
        .unwrap_or(false)
}

pub fn model_path(model_name: &str) -> Option<PathBuf> {
    AVAILABLE_MODELS.iter()
        .find(|m| m.name == model_name)
        .map(|info| models_dir().join(info.filename))
}

pub async fn download_model(
    model_name: &str,
    progress_callback: impl Fn(u64, u64),
) -> anyhow::Result<PathBuf> {
    let info = AVAILABLE_MODELS.iter()
        .find(|m| m.name == model_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown model: {}", model_name))?;

    let dir = models_dir();
    std::fs::create_dir_all(&dir)?;

    let dest = dir.join(info.filename);
    let url = format!("{}/{}", HF_BASE_URL, info.filename);

    let response = reqwest::get(&url).await?;
    let total_size = response.content_length().unwrap_or(0);

    use futures_util::StreamExt;
    use std::io::Write;
    let mut stream = response.bytes_stream();
    let mut file = std::fs::File::create(&dest)?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        progress_callback(downloaded, total_size);
    }

    Ok(dest)
}
```

**9. `src-tauri/src/transcription/whisper.rs`**

**Critical design:** `WhisperState` (created by `ctx.create_state()`) is NOT `Send + Sync`. It cannot be held across `.await` points or passed between threads. The solution is a **dedicated transcription thread** that owns the `WhisperContext` and communicates via `mpsc` channels.

```rust
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};
use std::path::Path;
use std::sync::mpsc;
use std::thread;

// --- Internal service (lives on dedicated thread) ---

struct TranscriptionService {
    context: Option<WhisperContext>,
}

impl TranscriptionService {
    fn new() -> Self { Self { context: None } }

    fn load_model(&mut self, model_path: &Path) -> anyhow::Result<()> {
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
            WhisperContextParameters::default(),
        ).map_err(|e| anyhow::anyhow!("Failed to load model: {:?}", e))?;
        self.context = Some(ctx);
        Ok(())
    }

    fn transcribe(&self, audio_data: &[f32]) -> anyhow::Result<String> {
        let ctx = self.context.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No model loaded"))?;

        let mut state = ctx.create_state()
            .map_err(|e| anyhow::anyhow!("Failed to create state: {:?}", e))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(4);
        params.set_translate(false);
        params.set_language(Some("en"));
        params.set_no_context(true);
        params.set_single_segment(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);

        state.full(params, audio_data)
            .map_err(|e| anyhow::anyhow!("Transcription failed: {:?}", e))?;

        let num_segments = state.full_n_segments()
            .map_err(|e| anyhow::anyhow!("Failed to get segments: {:?}", e))?;

        let mut text = String::new();
        for i in 0..num_segments {
            if let Ok(segment_text) = state.full_get_segment_text(i) {
                text.push_str(&segment_text);
            }
        }

        Ok(text.trim().to_string())
    }
}

// --- Public channel-based API ---

pub enum TranscriptionRequest {
    LoadModel(String),
    Transcribe(Vec<f32>),
    Shutdown,
}

pub enum TranscriptionResponse {
    ModelLoaded(Result<(), String>),
    TranscriptionComplete(Result<String, String>),
}

pub fn spawn_transcription_thread() -> (
    mpsc::Sender<TranscriptionRequest>,
    mpsc::Receiver<TranscriptionResponse>,
) {
    let (req_tx, req_rx) = mpsc::channel();
    let (resp_tx, resp_rx) = mpsc::channel();

    thread::spawn(move || {
        let mut service = TranscriptionService::new();
        while let Ok(request) = req_rx.recv() {
            match request {
                TranscriptionRequest::LoadModel(path) => {
                    let result = service.load_model(Path::new(&path));
                    let _ = resp_tx.send(TranscriptionResponse::ModelLoaded(
                        result.map_err(|e| e.to_string())
                    ));
                }
                TranscriptionRequest::Transcribe(audio) => {
                    let result = service.transcribe(&audio);
                    let _ = resp_tx.send(TranscriptionResponse::TranscriptionComplete(
                        result.map_err(|e| e.to_string())
                    ));
                }
                TranscriptionRequest::Shutdown => break,
            }
        }
    });

    (req_tx, resp_rx)
}
```

---

### Phase 4: Text Insertion

**10. `src-tauri/src/input/paste.rs`**

```rust
use arboard::Clipboard;
use enigo::{Enigo, Key, Keyboard, Direction, Settings};
use std::thread;
use std::time::Duration;

pub fn paste_text(text: &str) -> anyhow::Result<()> {
    // Write to clipboard
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text)?;

    // Small delay for clipboard readiness
    thread::sleep(Duration::from_millis(50));

    // Simulate Cmd+V
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Failed to create enigo: {:?}", e))?;

    enigo.key(Key::Meta, Direction::Press)
        .map_err(|e| anyhow::anyhow!("Key press failed: {:?}", e))?;
    enigo.key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| anyhow::anyhow!("Key click failed: {:?}", e))?;
    enigo.key(Key::Meta, Direction::Release)
        .map_err(|e| anyhow::anyhow!("Key release failed: {:?}", e))?;

    Ok(())
}

/// Check if Accessibility permission is granted (macOS)
#[cfg(target_os = "macos")]
pub fn check_accessibility_permission() -> bool {
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

#[cfg(not(target_os = "macos"))]
pub fn check_accessibility_permission() -> bool {
    true
}
```

---

### Phase 5: Core Orchestration

**11. `src-tauri/src/state.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use parking_lot::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum DictationState {
    Idle,
    Recording { duration_ms: u64 },
    Processing,
    Downloading { progress: f32 },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatePayload {
    pub state: DictationState,
}

pub struct AppState {
    pub dictation_state: DictationState,
    pub model_path: Option<String>,
    pub selected_model: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            dictation_state: DictationState::Idle,
            model_path: None,
            selected_model: "base.en".to_string(),
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
```

**12. `src-tauri/src/lib.rs`** -- Wire everything together

This is the most complex file. Key responsibilities:
- Register `Alt+Space` global shortcut
- `toggle_recording` command: manages the state machine transitions
- Emits `dictation-state` events to the React frontend
- On first run, auto-downloads `base.en` model
- Makes overlay window non-activating on macOS

The state machine flow in `toggle_recording`:
```
Idle:
  → Start cpal recording
  → Set state to Recording
  → Show overlay window (without stealing focus)

Recording:
  → Stop cpal recording, get audio buffer
  → Set state to Processing
  → Send audio to transcription thread
  → Spawn background thread to wait for result
  → On result: paste_text(), set state to Idle, hide overlay

Processing:
  → Ignore (already processing)

Error:
  → Reset to Idle, hide overlay
```

**Making the overlay non-activating on macOS:**
```rust
#[cfg(target_os = "macos")]
fn make_window_non_activating(window: &tauri::WebviewWindow) {
    use cocoa::appkit::{NSWindow, NSWindowCollectionBehavior};
    use cocoa::base::id;

    let ns_window = window.ns_window().unwrap() as id;
    unsafe {
        ns_window.setCollectionBehavior_(
            NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorIgnoresCycle
        );
        ns_window.setLevel_(cocoa::appkit::NSFloatingWindowLevel as i64);
    }
}
```

---

### Phase 6: Overlay UI

**13. `src/components/Overlay.tsx`**

Dark frosted-glass pill showing current state:

```tsx
import { useDictationState } from "../hooks/useDictationState";
import { PulseAnimation } from "./PulseAnimation";

export function Overlay() {
  const state = useDictationState();
  if (state.type === "Idle") return null;

  return (
    <div className="flex items-center gap-3 px-5 py-3 bg-black/80 backdrop-blur-xl
                    rounded-2xl border border-white/10 shadow-2xl select-none"
         data-tauri-drag-region>
      {state.type === "Recording" && (
        <>
          <PulseAnimation />
          <span className="text-red-400 text-sm font-medium">Listening...</span>
        </>
      )}
      {state.type === "Processing" && (
        <>
          <div className="w-4 h-4 border-2 border-blue-400 border-t-transparent
                          rounded-full animate-spin" />
          <span className="text-blue-400 text-sm font-medium">Transcribing...</span>
        </>
      )}
      {state.type === "Downloading" && (
        <>
          <div className="w-4 h-4 border-2 border-green-400 border-t-transparent
                          rounded-full animate-spin" />
          <span className="text-green-400 text-sm font-medium">
            Downloading model... {Math.round(state.progress)}%
          </span>
        </>
      )}
      {state.type === "Error" && (
        <>
          <div className="w-4 h-4 rounded-full bg-yellow-500" />
          <span className="text-yellow-400 text-xs">{state.message}</span>
        </>
      )}
    </div>
  );
}
```

**14. `src/hooks/useDictationState.ts`**

```tsx
import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

export type DictationState =
  | { type: "Idle" }
  | { type: "Recording"; duration_ms: number }
  | { type: "Processing" }
  | { type: "Downloading"; progress: number }
  | { type: "Error"; message: string };

export function useDictationState() {
  const [state, setState] = useState<DictationState>({ type: "Idle" });

  useEffect(() => {
    const unlisten = listen<{ state: DictationState }>("dictation-state", (event) => {
      setState(event.payload.state);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  return state;
}
```

**15. `src/components/PulseAnimation.tsx`**

```tsx
export function PulseAnimation() {
  return (
    <div className="relative w-4 h-4">
      <div className="absolute inset-0 rounded-full bg-red-500 animate-ping opacity-75" />
      <div className="relative w-4 h-4 rounded-full bg-red-500" />
    </div>
  );
}
```

---

## Key Technical Considerations

| Issue | Solution |
|-------|----------|
| `WhisperState` is not `Send`/`Sync` | Dedicated transcription thread with `mpsc` channels -- never crosses thread boundaries |
| Overlay stealing focus from target app | Non-activating `NSPanel` behavior via `cocoa` crate + `focus: false` in Tauri config |
| Option+Space conflict (Alfred/Raycast) | Document conflict for MVP; make hotkey configurable in future |
| Accessibility permission not granted | Check `AXIsProcessTrusted()` on startup, show overlay message if denied |
| First `cargo build` takes 2-5 minutes | whisper.cpp compiles from source via whisper-rs build script; subsequent builds are cached |
| App must be non-sandboxed | Required for CGEvent keyboard simulation; distribute via DMG not App Store |
| Audio buffer memory for long recordings | 16kHz mono f32: ~3.8MB/minute. 10 minutes = ~38MB. Not a concern for dictation. |

## Performance (base.en model, Apple Silicon with Metal)

| Recording Duration | Transcription Time |
|--------------------|--------------------|
| 5 seconds | ~0.3s |
| 30 seconds | ~1.5s |
| 60 seconds | ~3s |

## Model Storage

```
~/Library/Application Support/com.wren.app/
├── models/
│   ├── ggml-base.en.bin          (148 MB, auto-downloaded on first run)
│   └── ggml-small.en.bin         (488 MB, optional)
└── config.json                    (user preferences, future)
```

---

## Verification / Testing

1. `npm run tauri dev` -- app launches, overlay is hidden, no errors in console
2. First run: `base.en` model auto-downloads, overlay shows progress percentage
3. Press **Option+Space**: overlay appears with red pulsing dot + "Listening..."
4. Speak into microphone for a few seconds
5. Press **Option+Space** again: overlay shows spinner + "Transcribing..."
6. Transcribed text appears in the currently focused text field (e.g., TextEdit, VS Code, browser input)
7. Text is also on the clipboard (Cmd+V to paste again)
8. Overlay hides after paste completes
9. Verify in System Settings that Microphone and Accessibility permissions were prompted

---

## Future Enhancements (Not in MVP)

- Configurable hotkey (settings panel)
- Model selection UI
- Voice Activity Detection (Silero VAD) to auto-stop recording on silence
- Clipboard restoration (save + restore previous clipboard contents after paste)
- System tray icon with recording state indicator
- Multi-language support (remove `.en` model restriction)
- AI post-processing via local LLM (grammar cleanup, formatting)
- Audio input device selection
