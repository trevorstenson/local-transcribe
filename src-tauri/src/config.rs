use crate::translation::model_manager::DEFAULT_TRANSLATION_MODEL;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_HOTKEY: &str = "alt+space";
const DEFAULT_MODEL: &str = "base.en";
const DEFAULT_LANGUAGE: &str = "en";
const DEFAULT_TRANSLATION_TARGET_LANG: &str = "en";

fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

fn default_language() -> String {
    DEFAULT_LANGUAGE.to_string()
}

fn default_translation_target_lang() -> String {
    DEFAULT_TRANSLATION_TARGET_LANG.to_string()
}

fn default_translation_model() -> String {
    DEFAULT_TRANSLATION_MODEL.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub hotkey: String,
    #[serde(default = "default_model")]
    pub selected_model: String,
    #[serde(default = "default_true")]
    pub smart_paste: bool,
    #[serde(default)]
    pub overlay_x: Option<f64>,
    #[serde(default)]
    pub overlay_y: Option<f64>,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_true")]
    pub vocab_enabled: bool,
    #[serde(default)]
    pub translation_enabled: bool,
    #[serde(default = "default_translation_target_lang")]
    pub translation_target_lang: String,
    #[serde(default = "default_translation_model")]
    pub translation_model: String,
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.to_string(),
            selected_model: default_model(),
            smart_paste: true,
            overlay_x: None,
            overlay_y: None,
            language: default_language(),
            vocab_enabled: true,
            translation_enabled: false,
            translation_target_lang: default_translation_target_lang(),
            translation_model: default_translation_model(),
        }
    }
}

/// Returns the path to config.json in the app's data directory.
fn config_path() -> PathBuf {
    let data_dir = dirs::data_dir().expect("Failed to get data directory");
    data_dir.join("com.wren.app").join("config.json")
}

/// Reads the config from disk. Returns default config if file doesn't exist or is invalid.
pub fn load_config() -> AppConfig {
    let path = config_path();
    if !path.exists() {
        return AppConfig::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

/// Saves the config to disk. Creates the directory if needed.
pub fn save_config(config: &AppConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, contents)?;
    Ok(())
}
