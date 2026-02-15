use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MAX_HISTORY_ENTRIES: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: u64,
    pub text: String,
    pub timestamp_ms: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranscriptionHistory {
    pub entries: Vec<HistoryEntry>,
}

fn history_path() -> PathBuf {
    let data_dir = dirs::data_dir().expect("Failed to get data directory");
    data_dir.join("com.dictate.app").join("history.json")
}

pub fn load_history() -> TranscriptionHistory {
    let path = history_path();
    if !path.exists() {
        return TranscriptionHistory::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => TranscriptionHistory::default(),
    }
}

pub fn save_history(history: &TranscriptionHistory) -> Result<()> {
    let path = history_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(history)?;
    std::fs::write(&path, contents)?;
    Ok(())
}

pub fn add_entry(entry: HistoryEntry) -> Result<()> {
    let mut history = load_history();
    history.entries.insert(0, entry);
    if history.entries.len() > MAX_HISTORY_ENTRIES {
        history.entries.truncate(MAX_HISTORY_ENTRIES);
    }
    save_history(&history)
}

pub fn delete_entry(id: u64) -> Result<()> {
    let mut history = load_history();
    history.entries.retain(|e| e.id != id);
    save_history(&history)
}

pub fn clear_history() -> Result<()> {
    save_history(&TranscriptionHistory::default())
}
