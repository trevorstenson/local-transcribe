use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_HOTKEY: &str = "alt+space";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub hotkey: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.to_string(),
        }
    }
}

/// Returns the path to config.json in the app's data directory.
fn config_path() -> PathBuf {
    let data_dir = dirs::data_dir().expect("Failed to get data directory");
    data_dir
        .join("com.dictate.app")
        .join("config.json")
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
