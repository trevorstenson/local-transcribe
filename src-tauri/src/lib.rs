mod audio;
mod config;
mod input;
mod state;
mod transcription;

use state::{DictationState, SharedState, StatePayload};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use transcription::whisper::{TranscriptionRequest, TranscriptionResponse};

/// Makes the overlay window non-activating so it doesn't steal focus from the current app.
#[cfg(target_os = "macos")]
fn make_window_non_activating(window: &tauri::WebviewWindow) {
    use cocoa::appkit::{NSWindow, NSWindowCollectionBehavior};
    use cocoa::base::id;

    if let Ok(ns_window) = window.ns_window() {
        let ns_window = ns_window as id;
        unsafe {
            let behavior = NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorIgnoresCycle;
            ns_window.setCollectionBehavior_(behavior);

            // NSFloatingWindowLevel = 3
            ns_window.setLevel_(3);
        }
    }
}

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
            // Re-check accessibility permission before starting recording
            if !input::paste::check_accessibility_permission() {
                let error_state = DictationState::Error {
                    message: "Accessibility access needed — check System Settings > Privacy > Accessibility".to_string(),
                };
                {
                    let mut state = shared_state.lock();
                    state.dictation_state = error_state.clone();
                }
                emit_state(app_handle, &error_state);
                if let Some(window) = app_handle.get_webview_window("overlay") {
                    let _ = window.show();
                }
                return;
            }

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
                        let error_state = DictationState::Error {
                            message: "Microphone access needed — check System Settings > Privacy > Microphone".to_string(),
                        };
                        {
                            let mut state = shared_state.lock();
                            state.dictation_state = error_state.clone();
                        }
                        emit_state(app_handle, &error_state);
                        if let Some(window) = app_handle.get_webview_window("overlay") {
                            let _ = window.show();
                        }
                    }
                },
                Err(e) => {
                    log::error!("Failed to create audio capture: {}", e);
                    let error_state = DictationState::Error {
                        message: "Microphone access needed — check System Settings > Privacy > Microphone".to_string(),
                    };
                    {
                        let mut state = shared_state.lock();
                        state.dictation_state = error_state.clone();
                    }
                    emit_state(app_handle, &error_state);
                    if let Some(window) = app_handle.get_webview_window("overlay") {
                        let _ = window.show();
                    }
                }
            }
        }
        DictationState::Recording { .. } => {
            // Stop recording and begin transcription
            let audio_data = {
                let active_capture = app_handle.state::<ActiveCapture>();
                let mut ac = active_capture.0.lock().unwrap();
                if let Some(mut capture) = ac.take() {
                    capture.stop_recording()
                } else {
                    Vec::new()
                }
            };

            // If no audio data, just go back to Idle
            if audio_data.is_empty() {
                {
                    let mut state = shared_state.lock();
                    state.dictation_state = DictationState::Idle;
                }
                emit_state(app_handle, &DictationState::Idle);
                if let Some(window) = app_handle.get_webview_window("overlay") {
                    let _ = window.hide();
                }
                return;
            }

            // Set state to Processing
            {
                let mut state = shared_state.lock();
                state.dictation_state = DictationState::Processing;
            }
            emit_state(app_handle, &DictationState::Processing);

            // Send audio to transcription thread
            {
                let tx = app_handle.state::<TranscriptionSender>();
                let tx = tx.0.lock().unwrap();
                let _ = tx.send(TranscriptionRequest::Transcribe(audio_data));
            }

            // Spawn a thread to wait for the transcription result
            let app_handle_clone = app_handle.clone();
            std::thread::spawn(move || {
                let resp = {
                    let rx = app_handle_clone.state::<TranscriptionReceiver>();
                    let rx = rx.0.lock().unwrap();
                    rx.recv()
                };

                match resp {
                    Ok(TranscriptionResponse::TranscriptionComplete(Ok(text))) => {
                        let trimmed = text.trim().to_string();
                        if trimmed.is_empty() {
                            // Silent audio — go back to Idle without pasting
                            let shared_state = app_handle_clone.state::<SharedState>();
                            {
                                let mut state = shared_state.lock();
                                state.dictation_state = DictationState::Idle;
                            }
                            emit_state(&app_handle_clone, &DictationState::Idle);
                            if let Some(window) =
                                app_handle_clone.get_webview_window("overlay")
                            {
                                let _ = window.hide();
                            }
                        } else {
                            // Paste the transcribed text on the main thread
                            // (macOS requires AppKit/HID calls on the main thread)
                            let app_for_paste = app_handle_clone.clone();
                            let text_to_paste = trimmed.clone();
                            let _ = app_handle_clone.run_on_main_thread(move || {
                                if let Err(e) = input::paste::paste_text(&text_to_paste) {
                                    log::error!("Failed to paste text: {}", e);
                                    let error_state = DictationState::Error {
                                        message: format!("Failed to paste: {}", e),
                                    };
                                    let shared_state = app_for_paste.state::<SharedState>();
                                    {
                                        let mut state = shared_state.lock();
                                        state.dictation_state = error_state.clone();
                                    }
                                    emit_state(&app_for_paste, &error_state);
                                    return;
                                }

                                // Success — back to Idle
                                let shared_state = app_for_paste.state::<SharedState>();
                                {
                                    let mut state = shared_state.lock();
                                    state.dictation_state = DictationState::Idle;
                                }
                                emit_state(&app_for_paste, &DictationState::Idle);
                                if let Some(window) =
                                    app_for_paste.get_webview_window("overlay")
                                {
                                    let _ = window.hide();
                                }
                            });
                        }
                    }
                    Ok(TranscriptionResponse::TranscriptionComplete(Err(e))) => {
                        log::error!("Transcription error: {}", e);
                        let error_state = DictationState::Error {
                            message: format!("Transcription failed: {}", e),
                        };
                        let shared_state = app_handle_clone.state::<SharedState>();
                        {
                            let mut state = shared_state.lock();
                            state.dictation_state = error_state.clone();
                        }
                        emit_state(&app_handle_clone, &error_state);
                        // Keep overlay visible for error state
                    }
                    Ok(_) => {
                        // Unexpected response type
                        log::error!("Unexpected transcription response");
                    }
                    Err(e) => {
                        log::error!("Failed to receive transcription response: {}", e);
                        let error_state = DictationState::Error {
                            message: "Transcription thread disconnected".to_string(),
                        };
                        let shared_state = app_handle_clone.state::<SharedState>();
                        {
                            let mut state = shared_state.lock();
                            state.dictation_state = error_state.clone();
                        }
                        emit_state(&app_handle_clone, &error_state);
                    }
                }
            });
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

/// Downloads the model if needed and loads it into the transcription thread.
fn setup_model(app_handle: tauri::AppHandle) {
    let shared_state = app_handle.state::<SharedState>();
    let selected_model = {
        let state = shared_state.lock();
        state.selected_model.clone()
    };

    let needs_download = !transcription::model_manager::model_exists(&selected_model);

    if needs_download {
        // Show overlay and emit Downloading state
        {
            let mut state = shared_state.lock();
            state.dictation_state = DictationState::Downloading { progress: 0.0 };
        }
        emit_state(
            &app_handle,
            &DictationState::Downloading { progress: 0.0 },
        );
        if let Some(window) = app_handle.get_webview_window("overlay") {
            let _ = window.show();
        }

        // Run async download on a tokio runtime in a separate thread
        let app_handle_dl = app_handle.clone();
        let model_name = selected_model.clone();
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        let download_result = rt.block_on(async {
            let app_handle_progress = app_handle_dl.clone();
            transcription::model_manager::download_model(&model_name, move |downloaded, total| {
                let progress = if total > 0 {
                    downloaded as f32 / total as f32
                } else {
                    0.0
                };
                let dl_state = DictationState::Downloading { progress };
                emit_state(&app_handle_progress, &dl_state);
            })
            .await
        });

        match download_result {
            Ok(path) => {
                let path_str = path.to_string_lossy().to_string();
                load_model(&app_handle, &path_str, &selected_model);
            }
            Err(e) => {
                log::error!("Failed to download model: {}", e);
                let error_state = DictationState::Error {
                    message: format!("Model download failed: {}", e),
                };
                {
                    let mut state = shared_state.lock();
                    state.dictation_state = error_state.clone();
                }
                emit_state(&app_handle, &error_state);
            }
        }
    } else {
        // Model already exists, load it directly
        let path = transcription::model_manager::model_path(&selected_model)
            .expect("Model should exist");
        let path_str = path.to_string_lossy().to_string();
        load_model(&app_handle, &path_str, &selected_model);
    }
}

/// Stores the currently active hotkey string.
pub struct CurrentHotkey(pub std::sync::Mutex<String>);

#[tauri::command]
fn get_hotkey(current_hotkey: tauri::State<'_, CurrentHotkey>) -> String {
    current_hotkey.0.lock().unwrap().clone()
}

#[tauri::command]
fn set_hotkey(
    app: tauri::AppHandle,
    current_hotkey: tauri::State<'_, CurrentHotkey>,
    new_hotkey: String,
) -> Result<(), String> {
    let old_hotkey = current_hotkey.0.lock().unwrap().clone();

    // Unregister the old shortcut
    let gs = app.global_shortcut();
    if gs.is_registered(old_hotkey.as_str()) {
        gs.unregister(old_hotkey.as_str()).map_err(|e| format!("Failed to unregister old hotkey: {}", e))?;
    }

    // Register the new shortcut with our handler
    gs.on_shortcut(new_hotkey.as_str(), |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            toggle_recording(app);
        }
    })
    .map_err(|e| {
        // Re-register the old shortcut on failure
        let _ = gs.on_shortcut(old_hotkey.as_str(), |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_recording(app);
            }
        });
        format!("Failed to register new hotkey '{}': {}", new_hotkey, e)
    })?;

    // Save to config
    let cfg = config::AppConfig {
        hotkey: new_hotkey.clone(),
    };
    config::save_config(&cfg).map_err(|e| format!("Failed to save config: {}", e))?;

    // Update in-memory state
    *current_hotkey.0.lock().unwrap() = new_hotkey;

    Ok(())
}

/// Sends LoadModel request to transcription thread and waits for response.
fn load_model(app_handle: &tauri::AppHandle, path: &str, _model_name: &str) {
    let tx = app_handle.state::<TranscriptionSender>();
    {
        let tx = tx.0.lock().unwrap();
        let _ = tx.send(TranscriptionRequest::LoadModel(path.to_string()));
    }

    let resp = {
        let rx = app_handle.state::<TranscriptionReceiver>();
        let rx = rx.0.lock().unwrap();
        rx.recv()
    };

    let shared_state = app_handle.state::<SharedState>();
    match resp {
        Ok(TranscriptionResponse::ModelLoaded(Ok(()))) => {
            log::info!("Model loaded successfully");
            {
                let mut state = shared_state.lock();
                state.model_path = Some(path.to_string());
                state.dictation_state = DictationState::Idle;
            }
            emit_state(app_handle, &DictationState::Idle);
            if let Some(window) = app_handle.get_webview_window("overlay") {
                let _ = window.hide();
            }
        }
        Ok(TranscriptionResponse::ModelLoaded(Err(e))) => {
            log::error!("Failed to load model: {}", e);
            let error_state = DictationState::Error {
                message: format!("Failed to load model: {}", e),
            };
            {
                let mut state = shared_state.lock();
                state.dictation_state = error_state.clone();
            }
            emit_state(app_handle, &error_state);
        }
        _ => {
            log::error!("Unexpected response when loading model");
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load config for saved hotkey
    let app_config = config::load_config();
    let hotkey = app_config.hotkey.clone();

    // Create shared state
    let shared_state: SharedState = Arc::new(parking_lot::Mutex::new(state::AppState::default()));

    // Spawn transcription thread
    let (req_tx, resp_rx) = transcription::whisper::spawn_transcription_thread();

    tauri::Builder::default()
        .manage(shared_state)
        .manage(TranscriptionSender(std::sync::Mutex::new(req_tx)))
        .manage(TranscriptionReceiver(std::sync::Mutex::new(resp_rx)))
        .manage(ActiveCapture(std::sync::Mutex::new(None)))
        .manage(CurrentHotkey(std::sync::Mutex::new(hotkey.clone())))
        .invoke_handler(tauri::generate_handler![get_hotkey, set_hotkey])
        .setup(move |app| {
            // Register global shortcut plugin with saved hotkey
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_shortcuts([hotkey.as_str()])?
                    .with_handler(|app, _shortcut, event| {
                        if event.state == ShortcutState::Pressed {
                            toggle_recording(app);
                        }
                    })
                    .build(),
            )?;

            // Make overlay window non-activating (doesn't steal focus)
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("overlay") {
                make_window_non_activating(&window);
            }

            // Check accessibility permission on startup
            if !input::paste::check_accessibility_permission() {
                let app_handle = app.handle().clone();
                let error_state = DictationState::Error {
                    message: "Accessibility access needed — check System Settings > Privacy > Accessibility".to_string(),
                };
                let shared_state = app_handle.state::<SharedState>();
                {
                    let mut state = shared_state.lock();
                    state.dictation_state = error_state.clone();
                }
                emit_state(&app_handle, &error_state);
                if let Some(window) = app_handle.get_webview_window("overlay") {
                    let _ = window.show();
                }
            }

            // Download/load model on startup in a background thread
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                setup_model(app_handle);
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
