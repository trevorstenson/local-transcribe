mod audio;
mod config;
mod input;
mod state;
mod tray;
mod transcription;

use serde::Serialize;
use state::{DictationState, SharedState, StatePayload};
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Signals the streaming partial transcription loop to stop.
pub struct StreamingActive(pub Arc<AtomicBool>);

/// Wrapper for the partial transcription results channel.
pub struct PartialTranscriptionReceiver(
    pub std::sync::Mutex<std::sync::mpsc::Receiver<String>>,
);

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
                            state.dictation_state = DictationState::Recording {
                                duration_ms: 0,
                                partial_text: None,
                            };
                        }

                        emit_state(
                            app_handle,
                            &DictationState::Recording {
                                duration_ms: 0,
                                partial_text: None,
                            },
                        );

                        // Show overlay window without focus
                        if let Some(window) = app_handle.get_webview_window("overlay") {
                            let _ = window.show();
                        }

                        // Start the streaming partial transcription loop
                        let streaming_flag = app_handle.state::<StreamingActive>();
                        streaming_flag.0.store(true, Ordering::SeqCst);
                        let flag = Arc::clone(&streaming_flag.0);
                        let app_stream = app_handle.clone();

                        // Spawn audio level emitter (~30fps)
                        let flag_levels = Arc::clone(&streaming_flag.0);
                        let app_levels = app_handle.clone();
                        std::thread::spawn(move || {
                            while flag_levels.load(Ordering::SeqCst) {
                                let levels = {
                                    let active_capture = app_levels.state::<ActiveCapture>();
                                    let ac = active_capture.0.lock().unwrap();
                                    match ac.as_ref() {
                                        Some(capture) => {
                                            let buf = capture.buffer().lock().unwrap();
                                            audio::levels::compute_levels(
                                                &buf,
                                                capture.sample_rate(),
                                                48,
                                            )
                                        }
                                        None => break,
                                    }
                                };

                                let _ = app_levels.emit("audio-levels", &levels);
                                std::thread::sleep(std::time::Duration::from_millis(33));
                            }
                        });

                        std::thread::spawn(move || {
                            // Wait for initial audio to accumulate
                            std::thread::sleep(std::time::Duration::from_millis(500));

                            while flag.load(Ordering::SeqCst) {
                                let tick_start = std::time::Instant::now();

                                // Clone the audio buffer
                                let audio_data = {
                                    let active_capture = app_stream.state::<ActiveCapture>();
                                    let ac = active_capture.0.lock().unwrap();
                                    match ac.as_ref() {
                                        Some(capture) => capture.clone_buffer_resampled(),
                                        None => break,
                                    }
                                };

                                if audio_data.is_empty() {
                                    std::thread::sleep(std::time::Duration::from_millis(500));
                                    continue;
                                }

                                // Send partial transcription request
                                {
                                    let tx = app_stream.state::<TranscriptionSender>();
                                    let tx = tx.0.lock().unwrap();
                                    let _ = tx.send(TranscriptionRequest::TranscribePartial(
                                        audio_data,
                                    ));
                                }

                                // Wait for partial result on the dedicated channel
                                let resp = {
                                    let rx =
                                        app_stream.state::<PartialTranscriptionReceiver>();
                                    let rx = rx.0.lock().unwrap();
                                    rx.recv_timeout(std::time::Duration::from_millis(5000))
                                };

                                if !flag.load(Ordering::SeqCst) {
                                    break;
                                }

                                if let Ok(text) = resp {
                                    let shared_state = app_stream.state::<SharedState>();
                                    let new_state = {
                                        let mut state = shared_state.lock();
                                        if let DictationState::Recording {
                                            duration_ms: d,
                                            ..
                                        } = state.dictation_state
                                        {
                                            let partial = if text.trim().is_empty() {
                                                None
                                            } else {
                                                Some(text.trim().to_string())
                                            };
                                            state.dictation_state =
                                                DictationState::Recording {
                                                    duration_ms: d,
                                                    partial_text: partial,
                                                };
                                            Some(state.dictation_state.clone())
                                        } else {
                                            None
                                        }
                                    };

                                    if let Some(new_state) = new_state {
                                        emit_state(&app_stream, &new_state);
                                    }
                                }

                                // Sleep remaining time to hit ~1s interval
                                let elapsed = tick_start.elapsed();
                                if elapsed < std::time::Duration::from_millis(1000) {
                                    std::thread::sleep(
                                        std::time::Duration::from_millis(1000) - elapsed,
                                    );
                                }
                            }
                        });
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
            // Stop the streaming loop
            let streaming_flag = app_handle.state::<StreamingActive>();
            streaming_flag.0.store(false, Ordering::SeqCst);

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
                            let smart_paste = app_handle_clone.state::<SharedState>().lock().smart_paste;
                            let _ = app_handle_clone.run_on_main_thread(move || {
                                if let Err(e) = input::paste::paste_text(&text_to_paste, smart_paste) {
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

    // Save to config (preserve other settings)
    let mut cfg = config::load_config();
    cfg.hotkey = new_hotkey.clone();
    config::save_config(&cfg).map_err(|e| format!("Failed to save config: {}", e))?;

    // Update in-memory state
    *current_hotkey.0.lock().unwrap() = new_hotkey;

    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct ModelInfoPayload {
    name: String,
    size_mb: u32,
    description: String,
    downloaded: bool,
    selected: bool,
}

#[tauri::command]
fn get_models(shared_state: tauri::State<'_, SharedState>) -> Vec<ModelInfoPayload> {
    let state = shared_state.lock();
    let selected = &state.selected_model;

    transcription::model_manager::AVAILABLE_MODELS
        .iter()
        .map(|m| ModelInfoPayload {
            name: m.name.to_string(),
            size_mb: m.size_mb,
            description: m.description.to_string(),
            downloaded: transcription::model_manager::model_exists(m.name),
            selected: m.name == selected,
        })
        .collect()
}

#[tauri::command]
async fn select_model(app: tauri::AppHandle, model_name: String) -> Result<(), String> {
    // Validate model name
    let valid = transcription::model_manager::AVAILABLE_MODELS
        .iter()
        .any(|m| m.name == model_name);
    if !valid {
        return Err(format!("Unknown model: {}", model_name));
    }

    // Update selected_model in state
    {
        let shared_state = app.state::<SharedState>();
        let mut state = shared_state.lock();
        state.selected_model = model_name.clone();
    }

    // Persist to config
    let mut cfg = config::load_config();
    cfg.selected_model = model_name.clone();
    config::save_config(&cfg).map_err(|e| format!("Failed to save config: {}", e))?;

    // Download + load the model in a blocking thread
    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || {
        setup_model(app_clone);
    })
    .await
    .map_err(|e| format!("Model setup failed: {}", e))?;

    // Emit event so the settings UI can refresh
    let _ = app.emit("model-changed", ());

    Ok(())
}

#[tauri::command]
fn save_overlay_position(x: f64, y: f64) -> Result<(), String> {
    let mut cfg = config::load_config();
    cfg.overlay_x = Some(x);
    cfg.overlay_y = Some(y);
    config::save_config(&cfg).map_err(|e| format!("Failed to save position: {}", e))?;
    Ok(())
}

#[tauri::command]
fn get_smart_paste(shared_state: tauri::State<'_, SharedState>) -> bool {
    shared_state.lock().smart_paste
}

#[tauri::command]
fn set_smart_paste(
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<(), String> {
    // Update in-memory state
    {
        let shared_state = app.state::<SharedState>();
        let mut state = shared_state.lock();
        state.smart_paste = enabled;
    }

    // Persist to config
    let mut cfg = config::load_config();
    cfg.smart_paste = enabled;
    config::save_config(&cfg).map_err(|e| format!("Failed to save config: {}", e))?;

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
    // Load config for saved hotkey and model
    let app_config = config::load_config();
    let hotkey = app_config.hotkey.clone();
    let selected_model = app_config.selected_model.clone();
    let smart_paste = app_config.smart_paste;

    // Create shared state with persisted settings
    let shared_state: SharedState = Arc::new(parking_lot::Mutex::new(state::AppState {
        dictation_state: DictationState::Idle,
        model_path: None,
        selected_model,
        smart_paste,
    }));

    // Spawn transcription thread
    let (req_tx, resp_rx, partial_rx) = transcription::whisper::spawn_transcription_thread();

    tauri::Builder::default()
        .manage(shared_state)
        .manage(TranscriptionSender(std::sync::Mutex::new(req_tx)))
        .manage(TranscriptionReceiver(std::sync::Mutex::new(resp_rx)))
        .manage(PartialTranscriptionReceiver(std::sync::Mutex::new(partial_rx)))
        .manage(ActiveCapture(std::sync::Mutex::new(None)))
        .manage(StreamingActive(Arc::new(AtomicBool::new(false))))
        .manage(CurrentHotkey(std::sync::Mutex::new(hotkey.clone())))
        .invoke_handler(tauri::generate_handler![get_hotkey, set_hotkey, get_models, select_model, get_smart_paste, set_smart_paste, save_overlay_position])
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

            // Set up system tray icon
            tray::setup_tray(app)?;

            // Make overlay window non-activating (doesn't steal focus)
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("overlay") {
                make_window_non_activating(&window);
            }

            // Restore saved overlay position
            if let Some(window) = app.get_webview_window("overlay") {
                let cfg = config::load_config();
                if let (Some(x), Some(y)) = (cfg.overlay_x, cfg.overlay_y) {
                    let _ = window.set_position(tauri::Position::Logical(
                        tauri::LogicalPosition::new(x, y),
                    ));
                }
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
