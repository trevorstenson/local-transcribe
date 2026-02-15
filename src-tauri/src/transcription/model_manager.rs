use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use std::path::PathBuf;

pub struct ModelInfo {
    pub name: &'static str,
    pub filename: &'static str,
    pub size_mb: u32,
    pub description: &'static str,
}

pub const AVAILABLE_MODELS: [ModelInfo; 5] = [
    ModelInfo {
        name: "tiny.en",
        filename: "ggml-tiny.en.bin",
        size_mb: 78,
        description: "Tiny English-only model — fastest, least accurate",
    },
    ModelInfo {
        name: "base.en",
        filename: "ggml-base.en.bin",
        size_mb: 148,
        description: "Base English-only model — good balance of speed and accuracy",
    },
    ModelInfo {
        name: "small.en",
        filename: "ggml-small.en.bin",
        size_mb: 488,
        description: "Small English-only model — more accurate, slower",
    },
    ModelInfo {
        name: "medium.en",
        filename: "ggml-medium.en.bin",
        size_mb: 1530,
        description: "Medium English-only model — high accuracy, slow",
    },
    ModelInfo {
        name: "base.en-q8_0",
        filename: "ggml-base.en-q8_0.bin",
        size_mb: 82,
        description: "Base English-only quantized model — fast with good accuracy",
    },
];

const HF_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub fn models_dir() -> PathBuf {
    let data_dir = dirs::data_dir().expect("Could not determine data directory");
    data_dir.join("com.dictate.app").join("models")
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
    let model = find_model(model_name)
        .ok_or_else(|| anyhow!("Unknown model: {}", model_name))?;

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
