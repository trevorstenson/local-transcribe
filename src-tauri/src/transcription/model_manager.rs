use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use std::path::PathBuf;

pub struct ModelInfo {
    pub name: &'static str,
    pub filename: &'static str,
    pub size_mb: u32,
    pub description: &'static str,
    pub english_only: bool,
}

pub const AVAILABLE_MODELS: [ModelInfo; 9] = [
    ModelInfo {
        name: "tiny.en",
        filename: "ggml-tiny.en.bin",
        size_mb: 78,
        description: "Tiny English-only model — fastest, least accurate",
        english_only: true,
    },
    ModelInfo {
        name: "tiny",
        filename: "ggml-tiny.bin",
        size_mb: 78,
        description: "Tiny multilingual model — fastest, least accurate",
        english_only: false,
    },
    ModelInfo {
        name: "base.en",
        filename: "ggml-base.en.bin",
        size_mb: 148,
        description: "Base English-only model — good balance of speed and accuracy",
        english_only: true,
    },
    ModelInfo {
        name: "base",
        filename: "ggml-base.bin",
        size_mb: 148,
        description: "Base multilingual model — good balance of speed and accuracy",
        english_only: false,
    },
    ModelInfo {
        name: "small.en",
        filename: "ggml-small.en.bin",
        size_mb: 488,
        description: "Small English-only model — more accurate, slower",
        english_only: true,
    },
    ModelInfo {
        name: "small",
        filename: "ggml-small.bin",
        size_mb: 488,
        description: "Small multilingual model — more accurate, slower",
        english_only: false,
    },
    ModelInfo {
        name: "medium.en",
        filename: "ggml-medium.en.bin",
        size_mb: 1530,
        description: "Medium English-only model — high accuracy, slow",
        english_only: true,
    },
    ModelInfo {
        name: "medium",
        filename: "ggml-medium.bin",
        size_mb: 1530,
        description: "Medium multilingual model — high accuracy, slow",
        english_only: false,
    },
    ModelInfo {
        name: "base.en-q8_0",
        filename: "ggml-base.en-q8_0.bin",
        size_mb: 82,
        description: "Base English-only quantized model — fast with good accuracy",
        english_only: true,
    },
];

const HF_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub fn models_dir() -> PathBuf {
    let data_dir = dirs::data_dir().expect("Could not determine data directory");
    data_dir.join("com.wren.app").join("models")
}

fn find_model(model_name: &str) -> Option<&'static ModelInfo> {
    AVAILABLE_MODELS.iter().find(|m| m.name == model_name)
}

pub fn model_exists(model_name: &str) -> bool {
    match find_model(model_name) {
        Some(model) => models_dir().join(model.filename).exists(),
        None => false,
    }
}

pub fn model_path(model_name: &str) -> Option<PathBuf> {
    find_model(model_name).map(|model| models_dir().join(model.filename))
}

pub async fn download_model<F>(model_name: &str, progress_callback: F) -> Result<PathBuf>
where
    F: Fn(u64, u64),
{
    let model = find_model(model_name).ok_or_else(|| anyhow!("Unknown model: {}", model_name))?;

    let dir = models_dir();
    std::fs::create_dir_all(&dir)?;

    let dest = dir.join(model.filename);
    let url = format!("{}/{}", HF_BASE_URL, model.filename);

    let response = reqwest::get(&url).await?;

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let mut file = std::fs::File::create(&dest)?;
    let mut stream = response.bytes_stream();

    use std::io::Write;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        progress_callback(downloaded, total);
    }

    Ok(dest)
}

/// Returns the multilingual equivalent of an English-only model name.
pub fn multilingual_equivalent(model_name: &str) -> Option<&'static str> {
    match model_name {
        "tiny.en" => Some("tiny"),
        "base.en" | "base.en-q8_0" => Some("base"),
        "small.en" => Some("small"),
        "medium.en" => Some("medium"),
        _ => None,
    }
}

/// Returns the English-only equivalent of a multilingual model name.
pub fn english_equivalent(model_name: &str) -> Option<&'static str> {
    match model_name {
        "tiny" => Some("tiny.en"),
        "base" => Some("base.en"),
        "small" => Some("small.en"),
        "medium" => Some("medium.en"),
        _ => None,
    }
}

/// Returns whether a model is English-only.
pub fn is_english_only(model_name: &str) -> bool {
    find_model(model_name)
        .map(|m| m.english_only)
        .unwrap_or(false)
}
