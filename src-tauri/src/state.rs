use crate::vocabulary::CorrectionApplied;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DictationState {
    Idle,
    Recording {
        duration_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        partial_text: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        partial_translation: Option<String>,
        source_lang: String,
        target_lang: String,
    },
    Processing,
    Translating,
    Downloading {
        progress: f32,
    },
    Error {
        message: String,
    },
    CorrectionPreview {
        text: String,
        original_text: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        corrections: Vec<CorrectionApplied>,
    },
    TranslationPreview {
        source_text: String,
        translated_text: String,
        source_lang: String,
        target_lang: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatePayload {
    pub state: DictationState,
}

pub struct AppState {
    pub dictation_state: DictationState,
    pub model_path: Option<String>,
    pub selected_model: String,
    pub smart_paste: bool,
    pub language: String,
    pub vocab_enabled: bool,
    pub translation_enabled: bool,
    pub translation_target_lang: String,
    pub translation_model: String,
    pub pending_original_text: Option<String>,
    pub pending_corrected_text: Option<String>,
    pub pending_source_text: Option<String>,
    pub pending_translated_text: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            dictation_state: DictationState::Idle,
            model_path: None,
            selected_model: String::from("base.en"),
            smart_paste: true,
            language: String::from("en"),
            vocab_enabled: true,
            translation_enabled: false,
            translation_target_lang: String::from("en"),
            translation_model: String::from("nllb-200-distilled-600M-int8"),
            pending_original_text: None,
            pending_corrected_text: None,
            pending_source_text: None,
            pending_translated_text: None,
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
