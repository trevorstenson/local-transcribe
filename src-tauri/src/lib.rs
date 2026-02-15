mod audio;
mod input;
mod state;
mod transcription;

use state::{DictationState, SharedState, StatePayload};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::ShortcutState;
use transcription::whisper::{TranscriptionRequest, TranscriptionResponse};

/// Wrapper to store the transcription channel sender as managed state.
pub struct TranscriptionSender(pub std::sync::Mutex<std::sync::mpsc::Sender<TranscriptionRequest>>);

/// Wrapper to store the transcription channel receiver as managed state.
pub struct TranscriptionReceiver(
    pub std::sync::Mutex<std::sync::mpsc::Receiver<TranscriptionResponse>>,
);

/// Wrapper to store an active AudioCapture instance during recording.
pub struct ActiveCapture(pub std::sync::Mutex<Option<audio::capture::AudioCapture>>);

/// Emits the current dictation state to the frontend via a 'dictation-state' event.
fn emit_state(app_handle: &tauri::AppHandle, dictation_state: &DictationState) {
    let payload = StatePayload {
        state: dictation_state.clone(),
    };
    let _ = app_handle.emit("dictation-state", payload);
}

/// Toggles recording based on the current dictation state.
fn toggle_recording(app_handle: &tauri::AppHandle) {
    let shared_state = app_handle.state::<SharedState>();
    let current_state = {
        let state = shared_state.lock();
        state.dictation_state.clone()
    };

    match current_state {
        DictationState::Idle => {
            // Start recording
            match audio::capture::AudioCapture::new() {
                Ok(mut capture) => match capture.start_recording() {
                    Ok(()) => {
                        // Store the active capture
                        let active_capture = app_handle.state::<ActiveCapture>();
                        {
                            let mut ac = active_capture.0.lock().unwrap();
                            *ac = Some(capture);
                        }

                        // Update state to Recording
                        {
                            let mut state = shared_state.lock();
                            state.dictation_state =
                                DictationState::Recording { duration_ms: 0 };
                        }

                        emit_state(
                            app_handle,
                            &DictationState::Recording { duration_ms: 0 },
                        );

                        // Show overlay window without focus
                        if let Some(window) = app_handle.get_webview_window("overlay") {
                            let _ = window.show();
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to start recording: {}", e);
                    }
                },
                Err(e) => {
                    log::error!("Failed to create audio capture: {}", e);
                }
            }
        }
        DictationState::Recording { .. } => {
            // Stop recording
            let audio_data = {
                let active_capture = app_handle.state::<ActiveCapture>();
                let mut ac = active_capture.0.lock().unwrap();
                if let Some(mut capture) = ac.take() {
                    capture.stop_recording()
                } else {
                    Vec::new()
                }
            };

            // Update state to Idle (US-008 will change this to Processing)
            {
                let mut state = shared_state.lock();
                state.dictation_state = DictationState::Idle;
            }

            emit_state(app_handle, &DictationState::Idle);

            // Hide overlay window
            if let Some(window) = app_handle.get_webview_window("overlay") {
                let _ = window.hide();
            }

            // Audio data captured but not yet sent to transcription (US-008 handles this)
            let _ = audio_data;
        }
        DictationState::Processing | DictationState::Downloading { .. } => {
            // Ignore hotkey during processing or downloading
        }
        DictationState::Error { .. } => {
            // Reset to Idle on error
            {
                let mut state = shared_state.lock();
                state.dictation_state = DictationState::Idle;
            }

            emit_state(app_handle, &DictationState::Idle);

            // Hide overlay window
            if let Some(window) = app_handle.get_webview_window("overlay") {
                let _ = window.hide();
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Create shared state
    let shared_state: SharedState = Arc::new(parking_lot::Mutex::new(state::AppState::default()));

    // Spawn transcription thread
    let (req_tx, resp_rx) = transcription::whisper::spawn_transcription_thread();

    tauri::Builder::default()
        .manage(shared_state)
        .manage(TranscriptionSender(std::sync::Mutex::new(req_tx)))
        .manage(TranscriptionReceiver(std::sync::Mutex::new(resp_rx)))
        .manage(ActiveCapture(std::sync::Mutex::new(None)))
        .setup(|app| {
            // Register global shortcut plugin with Alt+Space
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_shortcuts(["alt+space"])?
                    .with_handler(|app, _shortcut, event| {
                        if event.state == ShortcutState::Pressed {
                            toggle_recording(app);
                        }
                    })
                    .build(),
            )?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
