use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocabEntry {
    pub id: u64,
    pub phrase: String,
    pub replacement: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Vocabulary {
    pub entries: Vec<VocabEntry>,
}

fn vocabulary_path() -> PathBuf {
    let data_dir = dirs::data_dir().expect("Failed to get data directory");
    data_dir
        .join("com.dictate.app")
        .join("vocabulary.json")
}

pub fn load_vocabulary() -> Vocabulary {
    let path = vocabulary_path();
    if !path.exists() {
        return Vocabulary::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Vocabulary::default(),
    }
}

pub fn save_vocabulary(vocabulary: &Vocabulary) -> Result<()> {
    let path = vocabulary_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(vocabulary)?;
    std::fs::write(&path, contents)?;
    Ok(())
}

pub fn add_entry(entry: VocabEntry) -> Result<()> {
    let mut vocabulary = load_vocabulary();
    vocabulary.entries.push(entry);
    save_vocabulary(&vocabulary)
}

pub fn update_entry(id: u64, phrase: String, replacement: String, enabled: bool) -> Result<()> {
    let mut vocabulary = load_vocabulary();
    if let Some(entry) = vocabulary.entries.iter_mut().find(|e| e.id == id) {
        entry.phrase = phrase;
        entry.replacement = replacement;
        entry.enabled = enabled;
    }
    save_vocabulary(&vocabulary)
}

pub fn delete_entry(id: u64) -> Result<()> {
    let mut vocabulary = load_vocabulary();
    vocabulary.entries.retain(|e| e.id != id);
    save_vocabulary(&vocabulary)
}
