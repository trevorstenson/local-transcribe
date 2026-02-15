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
    },
    Processing,
    Downloading { progress: f32 },
    Error { message: String },
    CorrectionPreview {
        text: String,
        original_text: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        corrections: Vec<CorrectionApplied>,
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
    pub pending_original_text: Option<String>,
    pub pending_corrected_text: Option<String>,
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
            pending_original_text: None,
            pending_corrected_text: None,
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
