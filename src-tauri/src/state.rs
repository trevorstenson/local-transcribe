use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            selected_model: String::from("base.en"),
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
