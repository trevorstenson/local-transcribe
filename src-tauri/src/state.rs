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
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
