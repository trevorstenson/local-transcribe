mod audio;
mod config;
mod history;
mod input;
mod state;
mod transcription;
mod translation;
mod tray;
mod vocabulary;

use serde::Serialize;
use state::{DictationState, SharedState, StatePayload};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use transcription::whisper::{TranscriptionRequest, TranscriptionResponse};
use translation::engine::{TranslationJob, TranslationRequest, TranslationResponse};

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

/// CGEventTap-based key interception for preview states.
/// Unlike NSEvent global monitors, a CGEventTap can suppress events so they
/// never reach the active application — preventing Enter from submitting forms
/// or Escape from closing dialogs while a preview overlay is visible.
#[cfg(target_os = "macos")]
mod preview_keys {
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::OnceLock;
    use tauri::Emitter;

    type CGEventRef = *mut c_void;
    type CGEventTapProxy = *mut c_void;

    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;
    const K_CG_EVENT_KEY_DOWN: u32 = 10;
    const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
    const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFFFFFE;

    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: extern "C" fn(CGEventTapProxy, u32, CGEventRef, *mut c_void) -> CGEventRef,
            user_info: *mut c_void,
        ) -> *mut c_void;
        fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
        fn CGEventTapEnable(tap: *mut c_void, enable: bool);
        fn CFMachPortCreateRunLoopSource(
            allocator: *const c_void,
            port: *mut c_void,
            order: isize,
        ) -> *mut c_void;
        fn CFRunLoopGetMain() -> *mut c_void;
        fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: *const c_void);
        static kCFRunLoopCommonModes: *const c_void;
    }

    static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

    struct TapRef(*mut c_void);
    unsafe impl Send for TapRef {}
    unsafe impl Sync for TapRef {}

    static EVENT_TAP: OnceLock<TapRef> = OnceLock::new();

    extern "C" fn tap_callback(
        _proxy: CGEventTapProxy,
        event_type: u32,
        event: CGEventRef,
        _user_info: *mut c_void,
    ) -> CGEventRef {
        if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT {
            if let Some(tap) = EVENT_TAP.get() {
                unsafe { CGEventTapEnable(tap.0, true); }
            }
            return event;
        }

        if event_type != K_CG_EVENT_KEY_DOWN {
            return event;
        }

        let key_code = unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) };
        match key_code {
            36 => {
                if let Some(app) = APP_HANDLE.get() {
                    let _ = app.emit("preview-key-pressed", "enter");
                }
                ptr::null_mut()
            }
            53 => {
                if let Some(app) = APP_HANDLE.get() {
                    let _ = app.emit("preview-key-pressed", "escape");
                }
                ptr::null_mut()
            }
            _ => event,
        }
    }

    /// Creates the event tap (initially disabled) and adds it to the main run loop.
    /// Must be called once during app setup.
    pub fn setup(app_handle: tauri::AppHandle) {
        APP_HANDLE.get_or_init(|| app_handle);

        unsafe {
            let mask: u64 = 1 << K_CG_EVENT_KEY_DOWN;
            let tap = CGEventTapCreate(
                K_CG_SESSION_EVENT_TAP,
                K_CG_HEAD_INSERT_EVENT_TAP,
                K_CG_EVENT_TAP_OPTION_DEFAULT,
                mask,
                tap_callback,
                ptr::null_mut(),
            );

            if tap.is_null() {
                log::error!("Failed to create CGEventTap for preview keys");
                return;
            }

            CGEventTapEnable(tap, false);

            let source = CFMachPortCreateRunLoopSource(ptr::null(), tap, 0);
            if !source.is_null() {
                CFRunLoopAddSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
            }

            EVENT_TAP.get_or_init(|| TapRef(tap));
        }
    }

    pub fn enable() {
        if let Some(tap) = EVENT_TAP.get() {
            unsafe { CGEventTapEnable(tap.0, true); }
        }
    }

    pub fn disable() {
        if let Some(tap) = EVENT_TAP.get() {
            unsafe { CGEventTapEnable(tap.0, false); }
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
pub struct PartialTranscriptionReceiver(pub std::sync::Mutex<std::sync::mpsc::Receiver<String>>);

/// Wrapper to store the translation channel sender as managed state.
pub struct TranslationSender(pub std::sync::Mutex<std::sync::mpsc::Sender<TranslationRequest>>);

/// Wrapper to store the translation channel receiver as managed state.
pub struct TranslationReceiver(
    pub std::sync::Mutex<std::sync::mpsc::Receiver<TranslationResponse>>,
);

/// Wrapper for the partial translation results channel.
pub struct PartialTranslationReceiver(pub std::sync::Mutex<std::sync::mpsc::Receiver<String>>);

/// Emits the current dictation state to the frontend via a 'dictation-state' event.
/// Also manages the global key monitor for preview states.
fn emit_state(app_handle: &tauri::AppHandle, dictation_state: &DictationState) {
    #[cfg(target_os = "macos")]
    match dictation_state {
        DictationState::CorrectionPreview { .. } | DictationState::TranslationPreview { .. } => {
            preview_keys::enable();
        }
        _ => {
            preview_keys::disable();
        }
    }

    let payload = StatePayload {
        state: dictation_state.clone(),
    };
    let _ = app_handle.emit("dictation-state", payload);
}

fn source_language_for_translation(language: &str) -> String {
    if language == "auto" {
        "auto".to_string()
    } else {
        language.to_string()
    }
}

fn sync_translation_languages(app: &tauri::AppHandle) {
    let (source, target) = {
        let shared_state = app.state::<SharedState>();
        let state = shared_state.lock();
        (
            if state.language == "auto" {
                None
            } else {
                Some(state.language.clone())
            },
            state.translation_target_lang.clone(),
        )
    };

    let tx = app.state::<TranslationSender>();
    let tx = tx.0.lock().unwrap();
    let _ = tx.send(TranslationRequest::SetLanguages { source, target });
}

/// Registers fallback shortcuts for opening windows when the tray icon is hidden by macOS.
fn register_fallback_shortcuts(app: &tauri::AppHandle) {
    let gs = app.global_shortcut();

    if let Err(e) = gs.on_shortcut("cmd+alt+,", |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            tray::show_settings_window(app);
        }
    }) {
        log::warn!("Failed to register fallback settings shortcut: {}", e);
    }

    if let Err(e) = gs.on_shortcut("cmd+alt+h", |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            tray::show_history_window(app);
        }
    }) {
        log::warn!("Failed to register fallback history shortcut: {}", e);
    }
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
                        let initial_recording_state = {
                            let mut state = shared_state.lock();
                            let source_lang = source_language_for_translation(&state.language);
                            let target_lang = state.translation_target_lang.clone();
                            state.dictation_state = DictationState::Recording {
                                duration_ms: 0,
                                partial_text: None,
                                partial_translation: None,
                                source_lang,
                                target_lang,
                            };
                            state.dictation_state.clone()
                        };

                        emit_state(app_handle, &initial_recording_state);

                        // Show overlay window without focus
                        if let Some(window) = app_handle.get_webview_window("overlay") {
                            let _ = window.show();
                        }

                        // Start the streaming partial transcription loop
                        let streaming_flag = app_handle.state::<StreamingActive>();
                        streaming_flag.0.store(true, Ordering::SeqCst);
                        let flag = Arc::clone(&streaming_flag.0);
                        let app_stream = app_handle.clone();

                        // Spawn audio level emitter (~30fps) + duration tracker
                        let flag_levels = Arc::clone(&streaming_flag.0);
                        let app_levels = app_handle.clone();
                        let recording_start = std::time::Instant::now();
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

                                // Update duration_ms in shared state
                                let elapsed_ms = recording_start.elapsed().as_millis() as u64;
                                let shared_state = app_levels.state::<SharedState>();
                                let new_state = {
                                    let mut state = shared_state.lock();
                                    if let DictationState::Recording {
                                        partial_text,
                                        partial_translation,
                                        source_lang,
                                        target_lang,
                                        ..
                                    } = &state.dictation_state
                                    {
                                        state.dictation_state = DictationState::Recording {
                                            duration_ms: elapsed_ms,
                                            partial_text: partial_text.clone(),
                                            partial_translation: partial_translation.clone(),
                                            source_lang: source_lang.clone(),
                                            target_lang: target_lang.clone(),
                                        };
                                        Some(state.dictation_state.clone())
                                    } else {
                                        None
                                    }
                                };
                                if let Some(new_state) = new_state {
                                    emit_state(&app_levels, &new_state);
                                }

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
                                    let _ = tx
                                        .send(TranscriptionRequest::TranscribePartial(audio_data));
                                }

                                // Wait for partial result on the dedicated channel
                                let resp = {
                                    let rx = app_stream.state::<PartialTranscriptionReceiver>();
                                    let rx = rx.0.lock().unwrap();
                                    rx.recv_timeout(std::time::Duration::from_millis(5000))
                                };

                                if !flag.load(Ordering::SeqCst) {
                                    break;
                                }

                                if let Ok(text) = resp {
                                    let partial_text = text.trim().to_string();
                                    let partial = if partial_text.is_empty() {
                                        None
                                    } else {
                                        Some(partial_text.clone())
                                    };

                                    let (
                                        recording_duration_ms,
                                        source_lang,
                                        target_lang,
                                        translation_enabled,
                                    ) = {
                                        let shared_state = app_stream.state::<SharedState>();
                                        let state = shared_state.lock();
                                        if let DictationState::Recording {
                                            duration_ms,
                                            source_lang,
                                            target_lang,
                                            ..
                                        } = &state.dictation_state
                                        {
                                            (
                                                Some(*duration_ms),
                                                source_lang.clone(),
                                                target_lang.clone(),
                                                state.translation_enabled,
                                            )
                                        } else {
                                            (None, String::new(), String::new(), false)
                                        }
                                    };

                                    let partial_translation = if translation_enabled
                                        && partial.is_some()
                                        && recording_duration_ms.is_some()
                                    {
                                        {
                                            let tx = app_stream.state::<TranslationSender>();
                                            let tx = tx.0.lock().unwrap();
                                            let _ = tx.send(TranslationRequest::TranslatePartial(
                                                TranslationJob {
                                                    text: partial_text.clone(),
                                                    source_lang: source_lang.clone(),
                                                    target_lang: target_lang.clone(),
                                                },
                                            ));
                                        }

                                        let resp = {
                                            let rx =
                                                app_stream.state::<PartialTranslationReceiver>();
                                            let rx = rx.0.lock().unwrap();
                                            rx.recv_timeout(std::time::Duration::from_millis(1500))
                                        };

                                        resp.ok().and_then(|t| {
                                            let trimmed = t.trim().to_string();
                                            if trimmed.is_empty() {
                                                None
                                            } else {
                                                Some(trimmed)
                                            }
                                        })
                                    } else {
                                        None
                                    };

                                    let shared_state = app_stream.state::<SharedState>();
                                    let new_state = {
                                        let mut state = shared_state.lock();
                                        if let DictationState::Recording {
                                            duration_ms: d,
                                            source_lang,
                                            target_lang,
                                            ..
                                        } = state.dictation_state.clone()
                                        {
                                            let partial = if text.trim().is_empty() {
                                                None
                                            } else {
                                                Some(text.trim().to_string())
                                            };
                                            state.dictation_state = DictationState::Recording {
                                                duration_ms: d,
                                                partial_text: partial,
                                                partial_translation,
                                                source_lang,
                                                target_lang,
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

            // Capture recording duration before transitioning to Processing
            let recording_duration_ms = {
                let state = shared_state.lock();
                if let DictationState::Recording { duration_ms, .. } = &state.dictation_state {
                    *duration_ms
                } else {
                    0
                }
            };

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

            // Spawn a thread to wait for the transcription result (with timeout)
            let app_handle_clone = app_handle.clone();
            std::thread::spawn(move || {
                let resp = {
                    let rx = app_handle_clone.state::<TranscriptionReceiver>();
                    let rx = rx.0.lock().unwrap();
                    rx.recv_timeout(std::time::Duration::from_secs(60))
                        .map_err(|_| ())
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
                            if let Some(window) = app_handle_clone.get_webview_window("overlay") {
                                let _ = window.hide();
                            }
                        } else {
                            let (
                                vocab_enabled,
                                translation_enabled,
                                source_lang,
                                target_lang,
                                smart_paste,
                            ) = {
                                let shared_state = app_handle_clone.state::<SharedState>();
                                let state = shared_state.lock();
                                (
                                    state.vocab_enabled,
                                    state.translation_enabled,
                                    source_language_for_translation(&state.language),
                                    state.translation_target_lang.clone(),
                                    state.smart_paste,
                                )
                            };

                            let correction_result = if vocab_enabled {
                                let vocab = vocabulary::load_vocabulary();
                                let result = vocabulary::apply_corrections(&trimmed, &vocab);
                                if result.corrections.is_empty() {
                                    None
                                } else {
                                    Some(result)
                                }
                            } else {
                                None
                            };

                            let source_text = correction_result
                                .as_ref()
                                .map_or_else(|| trimmed.clone(), |r| r.text.clone());

                            let timestamp_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;
                            let entry = history::HistoryEntry {
                                id: timestamp_ms,
                                text: source_text.clone(),
                                timestamp_ms,
                                duration_ms: recording_duration_ms,
                            };
                            if let Err(e) = history::add_entry(entry) {
                                log::error!("Failed to save history entry: {}", e);
                            }
                            let _ = app_handle_clone.emit("history-updated", ());

                            if translation_enabled {
                                {
                                    let shared_state = app_handle_clone.state::<SharedState>();
                                    let mut state = shared_state.lock();
                                    state.dictation_state = DictationState::Translating;
                                }
                                emit_state(&app_handle_clone, &DictationState::Translating);

                                {
                                    let tx = app_handle_clone.state::<TranslationSender>();
                                    let tx = tx.0.lock().unwrap();
                                    let _ =
                                        tx.send(TranslationRequest::Translate(TranslationJob {
                                            text: source_text.clone(),
                                            source_lang: source_lang.clone(),
                                            target_lang: target_lang.clone(),
                                        }));
                                }

                                let translation_resp = {
                                    let rx = app_handle_clone.state::<TranslationReceiver>();
                                    let rx = rx.0.lock().unwrap();
                                    rx.recv_timeout(std::time::Duration::from_secs(30))
                                        .map_err(|_| ())
                                };

                                match translation_resp {
                                    Ok(TranslationResponse::TranslationComplete(Ok(
                                        translated,
                                    ))) => {
                                        let translated_text = translated.trim().to_string();
                                        let translated_text = if translated_text.is_empty() {
                                            source_text.clone()
                                        } else {
                                            translated_text
                                        };

                                        let preview_state = DictationState::TranslationPreview {
                                            source_text: source_text.clone(),
                                            translated_text: translated_text.clone(),
                                            source_lang: source_lang.clone(),
                                            target_lang: target_lang.clone(),
                                        };

                                        let shared_state = app_handle_clone.state::<SharedState>();
                                        {
                                            let mut state = shared_state.lock();
                                            state.pending_source_text = Some(source_text);
                                            state.pending_translated_text = Some(translated_text);
                                            state.dictation_state = preview_state.clone();
                                        }
                                        emit_state(&app_handle_clone, &preview_state);
                                    }
                                    Ok(TranslationResponse::TranslationComplete(Err(e))) => {
                                        log::error!("Translation failed: {}", e);
                                        let app_for_paste = app_handle_clone.clone();
                                        let text_to_paste = source_text.clone();
                                        let _ = app_handle_clone.run_on_main_thread(move || {
                                            if let Err(e) = input::paste::paste_text(
                                                &text_to_paste,
                                                smart_paste,
                                            ) {
                                                log::error!("Failed to paste text: {}", e);
                                                let error_state = DictationState::Error {
                                                    message: format!("Failed to paste: {}", e),
                                                };
                                                let shared_state =
                                                    app_for_paste.state::<SharedState>();
                                                {
                                                    let mut state = shared_state.lock();
                                                    state.dictation_state = error_state.clone();
                                                }
                                                emit_state(&app_for_paste, &error_state);
                                                return;
                                            }

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
                                    Ok(_) | Err(_) => {
                                        log::error!("Translation timed out or thread disconnected");
                                        let app_for_paste = app_handle_clone.clone();
                                        let text_to_paste = source_text.clone();
                                        let _ = app_handle_clone.run_on_main_thread(move || {
                                            if let Err(e) = input::paste::paste_text(
                                                &text_to_paste,
                                                smart_paste,
                                            ) {
                                                log::error!("Failed to paste text: {}", e);
                                                let error_state = DictationState::Error {
                                                    message: format!("Failed to paste: {}", e),
                                                };
                                                let shared_state =
                                                    app_for_paste.state::<SharedState>();
                                                {
                                                    let mut state = shared_state.lock();
                                                    state.dictation_state = error_state.clone();
                                                }
                                                emit_state(&app_for_paste, &error_state);
                                                return;
                                            }

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
                            } else if let Some(correction_result) = correction_result {
                                // Corrections found — show preview, do NOT paste yet
                                let preview_state = DictationState::CorrectionPreview {
                                    text: correction_result.text.clone(),
                                    original_text: trimmed.clone(),
                                    corrections: correction_result.corrections,
                                };
                                let shared_state = app_handle_clone.state::<SharedState>();
                                {
                                    let mut state = shared_state.lock();
                                    state.pending_original_text = Some(trimmed);
                                    state.pending_corrected_text = Some(correction_result.text);
                                    state.dictation_state = preview_state.clone();
                                }
                                emit_state(&app_handle_clone, &preview_state);
                            } else {
                                // No corrections — paste immediately.
                                let app_for_paste = app_handle_clone.clone();
                                let text_to_paste = source_text.clone();
                                let _ = app_handle_clone.run_on_main_thread(move || {
                                    if let Err(e) =
                                        input::paste::paste_text(&text_to_paste, smart_paste)
                                    {
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
                    Err(_) => {
                        log::error!("Transcription timed out or thread disconnected");
                        let error_state = DictationState::Error {
                            message: "Transcription timed out — try again".to_string(),
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
        DictationState::Processing
        | DictationState::Translating
        | DictationState::Downloading { .. }
        | DictationState::CorrectionPreview { .. }
        | DictationState::TranslationPreview { .. } => {
            // Ignore hotkey during processing, translating, downloading, or preview states
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
        emit_state(&app_handle, &DictationState::Downloading { progress: 0.0 });
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
        let path =
            transcription::model_manager::model_path(&selected_model).expect("Model should exist");
        let path_str = path.to_string_lossy().to_string();
        load_model(&app_handle, &path_str, &selected_model);
    }
}

fn setup_translation_model(app_handle: tauri::AppHandle) -> Result<(), String> {
    let model_name = {
        let shared_state = app_handle.state::<SharedState>();
        let state = shared_state.lock();
        state.translation_model.clone()
    };

    let model_path = if translation::model_manager::model_exists(&model_name) {
        translation::model_manager::model_path(&model_name)
    } else {
        log::info!(
            "Translation model '{}' not found; downloading from Hugging Face",
            model_name
        );

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                return Err(format!(
                    "Failed to create runtime for translation download: {}",
                    e
                ));
            }
        };

        match rt.block_on(async {
            translation::model_manager::download_model(&model_name, |_downloaded, _total| {}).await
        }) {
            Ok(path) => path,
            Err(e) => {
                return Err(format!("Failed to download translation model: {}", e));
            }
        }
    };

    let model_path_str = model_path.to_string_lossy().to_string();

    {
        let tx = app_handle.state::<TranslationSender>();
        let tx = tx.0.lock().unwrap();
        let _ = tx.send(TranslationRequest::LoadModel(Some(model_path_str)));
    }

    let resp = {
        let rx = app_handle.state::<TranslationReceiver>();
        let rx = rx.0.lock().unwrap();
        rx.recv_timeout(std::time::Duration::from_secs(60))
            .map_err(|_| ())
    };

    match resp {
        Ok(TranslationResponse::ModelLoaded(Ok(()))) => {
            log::info!("Translation model initialized");
            Ok(())
        }
        Ok(TranslationResponse::ModelLoaded(Err(e))) => {
            log::error!("Failed to initialize translation model: {}", e);
            Err(format!("Failed to initialize translation model: {}", e))
        }
        Ok(_) => {
            log::error!("Unexpected translation response during model initialization");
            Err("Unexpected translation response during model initialization".to_string())
        }
        Err(_) => {
            log::error!("Translation model initialization timed out");
            Err("Translation model initialization timed out".to_string())
        }
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
        gs.unregister(old_hotkey.as_str())
            .map_err(|e| format!("Failed to unregister old hotkey: {}", e))?;
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
    english_only: bool,
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
            english_only: m.english_only,
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
fn set_smart_paste(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
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

#[tauri::command]
fn get_vocab_enabled(shared_state: tauri::State<'_, SharedState>) -> bool {
    shared_state.lock().vocab_enabled
}

#[tauri::command]
fn set_vocab_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    // Update in-memory state
    {
        let shared_state = app.state::<SharedState>();
        let mut state = shared_state.lock();
        state.vocab_enabled = enabled;
    }

    // Persist to config
    let mut cfg = config::load_config();
    cfg.vocab_enabled = enabled;
    config::save_config(&cfg).map_err(|e| format!("Failed to save config: {}", e))?;

    Ok(())
}

#[tauri::command]
fn get_translation_enabled(shared_state: tauri::State<'_, SharedState>) -> bool {
    shared_state.lock().translation_enabled
}

#[tauri::command]
fn set_translation_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        setup_translation_model(app.clone())?;
    }

    {
        let shared_state = app.state::<SharedState>();
        let mut state = shared_state.lock();
        state.translation_enabled = enabled;
    }

    let mut cfg = config::load_config();
    cfg.translation_enabled = enabled;
    config::save_config(&cfg).map_err(|e| format!("Failed to save config: {}", e))?;

    sync_translation_languages(&app);
    Ok(())
}

#[tauri::command]
fn get_translation_target_lang(shared_state: tauri::State<'_, SharedState>) -> String {
    shared_state.lock().translation_target_lang.clone()
}

#[tauri::command]
fn set_translation_target_lang(app: tauri::AppHandle, target_lang: String) -> Result<(), String> {
    let updated_state = {
        let shared_state = app.state::<SharedState>();
        let mut state = shared_state.lock();
        state.translation_target_lang = target_lang.clone();
        if let DictationState::Recording {
            duration_ms,
            partial_text,
            partial_translation,
            source_lang,
            ..
        } = state.dictation_state.clone()
        {
            state.dictation_state = DictationState::Recording {
                duration_ms,
                partial_text,
                partial_translation,
                source_lang,
                target_lang: target_lang.clone(),
            };
            Some(state.dictation_state.clone())
        } else {
            None
        }
    };

    let mut cfg = config::load_config();
    cfg.translation_target_lang = target_lang;
    config::save_config(&cfg).map_err(|e| format!("Failed to save config: {}", e))?;

    sync_translation_languages(&app);
    if let Some(state) = updated_state {
        emit_state(&app, &state);
    }
    Ok(())
}

#[tauri::command]
fn get_vocabulary() -> Vec<vocabulary::VocabEntry> {
    vocabulary::load_vocabulary().entries
}

#[tauri::command]
fn add_vocab_entry(phrase: String, replacement: String) -> Result<(), String> {
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let entry = vocabulary::VocabEntry {
        id: timestamp_ms,
        phrase,
        replacement,
        enabled: true,
    };
    vocabulary::add_entry(entry).map_err(|e| format!("Failed to add vocab entry: {}", e))
}

#[tauri::command]
fn update_vocab_entry(
    id: u64,
    phrase: String,
    replacement: String,
    enabled: bool,
) -> Result<(), String> {
    vocabulary::update_entry(id, phrase, replacement, enabled)
        .map_err(|e| format!("Failed to update vocab entry: {}", e))
}

#[tauri::command]
fn delete_vocab_entry(id: u64) -> Result<(), String> {
    vocabulary::delete_entry(id).map_err(|e| format!("Failed to delete vocab entry: {}", e))
}

#[tauri::command]
fn accept_corrections(
    app: tauri::AppHandle,
    shared_state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    let (corrected_text, smart_paste) = {
        let mut state = shared_state.lock();
        let text = state
            .pending_corrected_text
            .take()
            .ok_or_else(|| "No pending corrections to accept".to_string())?;
        state.pending_original_text = None;
        let sp = state.smart_paste;
        (text, sp)
    };

    let app_for_paste = app.clone();
    let text_to_paste = corrected_text;
    app.run_on_main_thread(move || {
        if let Err(e) = input::paste::paste_text(&text_to_paste, smart_paste) {
            log::error!("Failed to paste corrected text: {}", e);
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

        let shared_state = app_for_paste.state::<SharedState>();
        {
            let mut state = shared_state.lock();
            state.dictation_state = DictationState::Idle;
        }
        emit_state(&app_for_paste, &DictationState::Idle);
        if let Some(window) = app_for_paste.get_webview_window("overlay") {
            let _ = window.hide();
        }
    })
    .map_err(|e| format!("Failed to run on main thread: {}", e))?;

    Ok(())
}

#[tauri::command]
fn undo_corrections(
    app: tauri::AppHandle,
    shared_state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    let (original_text, smart_paste) = {
        let mut state = shared_state.lock();
        let text = state
            .pending_original_text
            .take()
            .ok_or_else(|| "No pending corrections to undo".to_string())?;
        state.pending_corrected_text = None;
        let sp = state.smart_paste;
        (text, sp)
    };

    // Update the most recent history entry to use the original text
    if let Err(e) = history::update_most_recent_text(original_text.clone()) {
        log::error!("Failed to update history entry: {}", e);
    }
    let _ = app.emit("history-updated", ());

    let app_for_paste = app.clone();
    let text_to_paste = original_text;
    app.run_on_main_thread(move || {
        if let Err(e) = input::paste::paste_text(&text_to_paste, smart_paste) {
            log::error!("Failed to paste original text: {}", e);
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

        let shared_state = app_for_paste.state::<SharedState>();
        {
            let mut state = shared_state.lock();
            state.dictation_state = DictationState::Idle;
        }
        emit_state(&app_for_paste, &DictationState::Idle);
        if let Some(window) = app_for_paste.get_webview_window("overlay") {
            let _ = window.hide();
        }
    })
    .map_err(|e| format!("Failed to run on main thread: {}", e))?;

    Ok(())
}

#[tauri::command]
fn accept_translation(
    app: tauri::AppHandle,
    shared_state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    let (translated_text, smart_paste) = {
        let mut state = shared_state.lock();
        let text = state
            .pending_translated_text
            .take()
            .ok_or_else(|| "No pending translation to accept".to_string())?;
        state.pending_source_text = None;
        (text, state.smart_paste)
    };

    if let Err(e) = history::update_most_recent_text(translated_text.clone()) {
        log::error!("Failed to update history entry: {}", e);
    }
    let _ = app.emit("history-updated", ());

    let app_for_paste = app.clone();
    app.run_on_main_thread(move || {
        if let Err(e) = input::paste::paste_text(&translated_text, smart_paste) {
            log::error!("Failed to paste translated text: {}", e);
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

        let shared_state = app_for_paste.state::<SharedState>();
        {
            let mut state = shared_state.lock();
            state.dictation_state = DictationState::Idle;
        }
        emit_state(&app_for_paste, &DictationState::Idle);
        if let Some(window) = app_for_paste.get_webview_window("overlay") {
            let _ = window.hide();
        }
    })
    .map_err(|e| format!("Failed to run on main thread: {}", e))?;

    Ok(())
}

#[tauri::command]
fn reject_translation(
    app: tauri::AppHandle,
    shared_state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    let (source_text, smart_paste) = {
        let mut state = shared_state.lock();
        let text = state
            .pending_source_text
            .take()
            .ok_or_else(|| "No pending translation to reject".to_string())?;
        state.pending_translated_text = None;
        (text, state.smart_paste)
    };

    let app_for_paste = app.clone();
    app.run_on_main_thread(move || {
        if let Err(e) = input::paste::paste_text(&source_text, smart_paste) {
            log::error!("Failed to paste source text: {}", e);
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

        let shared_state = app_for_paste.state::<SharedState>();
        {
            let mut state = shared_state.lock();
            state.dictation_state = DictationState::Idle;
        }
        emit_state(&app_for_paste, &DictationState::Idle);
        if let Some(window) = app_for_paste.get_webview_window("overlay") {
            let _ = window.hide();
        }
    })
    .map_err(|e| format!("Failed to run on main thread: {}", e))?;

    Ok(())
}

#[tauri::command]
fn get_language(shared_state: tauri::State<'_, SharedState>) -> String {
    shared_state.lock().language.clone()
}

#[tauri::command]
async fn set_language(app: tauri::AppHandle, language: String) -> Result<(), String> {
    // Check if we need to switch between English-only and multilingual models
    let (current_model, needs_model_switch) = {
        let shared_state = app.state::<SharedState>();
        let state = shared_state.lock();
        let current = state.selected_model.clone();
        let is_en = transcription::model_manager::is_english_only(&current);
        let needs_multilingual = language != "en";
        (
            current.clone(),
            (needs_multilingual && is_en) || (!needs_multilingual && !is_en),
        )
    };

    // Determine new model if switching is needed
    let new_model = if needs_model_switch {
        if language != "en" {
            transcription::model_manager::multilingual_equivalent(&current_model)
                .unwrap_or("base")
                .to_string()
        } else {
            transcription::model_manager::english_equivalent(&current_model)
                .unwrap_or("base.en")
                .to_string()
        }
    } else {
        current_model.clone()
    };

    // Update language in state
    let updated_state = {
        let shared_state = app.state::<SharedState>();
        let mut state = shared_state.lock();
        state.language = language.clone();
        if let DictationState::Recording {
            duration_ms,
            partial_text,
            partial_translation,
            target_lang,
            ..
        } = state.dictation_state.clone()
        {
            state.dictation_state = DictationState::Recording {
                duration_ms,
                partial_text,
                partial_translation,
                source_lang: source_language_for_translation(&language),
                target_lang,
            };
            Some(state.dictation_state.clone())
        } else {
            None
        }
    };

    // Persist to config
    let mut cfg = config::load_config();
    cfg.language = language.clone();
    config::save_config(&cfg).map_err(|e| format!("Failed to save config: {}", e))?;

    // Send language to transcription thread
    let lang_for_whisper = if language == "auto" {
        None
    } else {
        Some(language)
    };
    {
        let tx = app.state::<TranscriptionSender>();
        let tx = tx.0.lock().unwrap();
        let _ = tx.send(TranscriptionRequest::SetLanguage(lang_for_whisper));
    }
    sync_translation_languages(&app);
    if let Some(state) = updated_state {
        emit_state(&app, &state);
    }

    // If model needs switching, trigger model change
    if needs_model_switch {
        select_model(app, new_model).await?;
    }

    Ok(())
}

#[tauri::command]
fn cancel_recording(app: tauri::AppHandle) {
    let shared_state = app.state::<SharedState>();
    let current_state = {
        let state = shared_state.lock();
        state.dictation_state.clone()
    };

    match current_state {
        DictationState::Recording { .. } => {
            // Stop the streaming loop
            let streaming_flag = app.state::<StreamingActive>();
            streaming_flag.0.store(false, Ordering::SeqCst);

            // Stop recording and discard audio
            {
                let active_capture = app.state::<ActiveCapture>();
                let mut ac = active_capture.0.lock().unwrap();
                if let Some(mut capture) = ac.take() {
                    let _ = capture.stop_recording();
                }
            }

            // Reset to Idle
            {
                let mut state = shared_state.lock();
                state.dictation_state = DictationState::Idle;
            }
            emit_state(&app, &DictationState::Idle);
            if let Some(window) = app.get_webview_window("overlay") {
                let _ = window.hide();
            }
        }
        DictationState::Error { .. } => {
            // Dismiss error
            {
                let mut state = shared_state.lock();
                state.dictation_state = DictationState::Idle;
            }
            emit_state(&app, &DictationState::Idle);
            if let Some(window) = app.get_webview_window("overlay") {
                let _ = window.hide();
            }
        }
        _ => {}
    }
}

#[tauri::command]
fn get_history() -> Result<Vec<history::HistoryEntry>, String> {
    Ok(history::load_history().entries)
}

#[tauri::command]
fn delete_history_entry(id: u64) -> Result<(), String> {
    history::delete_entry(id).map_err(|e| format!("Failed to delete entry: {}", e))
}

#[tauri::command]
fn clear_history() -> Result<(), String> {
    history::clear_history().map_err(|e| format!("Failed to clear history: {}", e))
}

#[tauri::command]
fn copy_history_entry(text: String) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("Failed to copy: {}", e))?;
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
        rx.recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|_| ())
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
        Err(_) => {
            log::error!("Model loading timed out or thread disconnected");
            let error_state = DictationState::Error {
                message: "Model loading timed out — try again".to_string(),
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
    let vocab_enabled = app_config.vocab_enabled;
    let language = app_config.language.clone();
    let translation_enabled = app_config.translation_enabled;
    let translation_target_lang = app_config.translation_target_lang.clone();
    let translation_model = app_config.translation_model.clone();

    // Create shared state with persisted settings
    let shared_state: SharedState = Arc::new(parking_lot::Mutex::new(state::AppState {
        dictation_state: DictationState::Idle,
        model_path: None,
        selected_model,
        smart_paste,
        language: language.clone(),
        vocab_enabled,
        translation_enabled,
        translation_target_lang,
        translation_model,
        pending_original_text: None,
        pending_corrected_text: None,
        pending_source_text: None,
        pending_translated_text: None,
    }));

    // Spawn transcription thread
    let (req_tx, resp_rx, partial_rx) = transcription::whisper::spawn_transcription_thread();
    let (translation_req_tx, translation_resp_rx, partial_translation_rx) =
        translation::engine::spawn_translation_thread();

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(shared_state)
        .manage(TranscriptionSender(std::sync::Mutex::new(req_tx)))
        .manage(TranscriptionReceiver(std::sync::Mutex::new(resp_rx)))
        .manage(PartialTranscriptionReceiver(std::sync::Mutex::new(partial_rx)))
        .manage(TranslationSender(std::sync::Mutex::new(translation_req_tx)))
        .manage(TranslationReceiver(std::sync::Mutex::new(translation_resp_rx)))
        .manage(PartialTranslationReceiver(std::sync::Mutex::new(
            partial_translation_rx,
        )))
        .manage(ActiveCapture(std::sync::Mutex::new(None)))
        .manage(StreamingActive(Arc::new(AtomicBool::new(false))))
        .manage(CurrentHotkey(std::sync::Mutex::new(hotkey.clone())))
        .invoke_handler(tauri::generate_handler![
            get_hotkey,
            set_hotkey,
            get_models,
            select_model,
            get_smart_paste,
            set_smart_paste,
            get_vocab_enabled,
            set_vocab_enabled,
            get_translation_enabled,
            set_translation_enabled,
            get_translation_target_lang,
            set_translation_target_lang,
            get_vocabulary,
            add_vocab_entry,
            update_vocab_entry,
            delete_vocab_entry,
            accept_corrections,
            undo_corrections,
            accept_translation,
            reject_translation,
            get_language,
            set_language,
            save_overlay_position,
            cancel_recording,
            get_history,
            delete_history_entry,
            clear_history,
            copy_history_entry
        ])
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

            // Register fallback window shortcuts for small-screen menu bar overflow.
            register_fallback_shortcuts(&app.handle());

            // Set up system tray icon
            tray::setup_tray(app)?;

            // Make overlay window non-activating (doesn't steal focus)
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("overlay") {
                make_window_non_activating(&window);
            }

            // Set up CGEventTap for preview key interception (Enter/Escape)
            #[cfg(target_os = "macos")]
            preview_keys::setup(app.handle().clone());

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

            // Send initial language to transcription thread
            {
                let tx = app.state::<TranscriptionSender>();
                let tx = tx.0.lock().unwrap();
                let lang_for_whisper = if language == "auto" {
                    None
                } else {
                    Some(language.clone())
                };
                let _ = tx.send(TranscriptionRequest::SetLanguage(lang_for_whisper));
            }
            sync_translation_languages(&app.handle());

            // Download/load model on startup in a background thread
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                setup_model(app_handle);
            });

            if translation_enabled {
                // Initialize translation backend on startup when translation is enabled.
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    if let Err(e) = setup_translation_model(app_handle) {
                        log::error!("{}", e);
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
