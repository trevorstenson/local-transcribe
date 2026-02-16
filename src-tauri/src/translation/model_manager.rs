use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use std::path::{Path, PathBuf};

pub const DEFAULT_TRANSLATION_MODEL: &str = "nllb-200-distilled-600M-int8";
const HF_BASE_URL: &str = "https://huggingface.co";

pub struct TranslationModelInfo {
    pub name: &'static str,
    pub repo: &'static str,
    pub required_files: &'static [&'static str],
}

pub const AVAILABLE_TRANSLATION_MODELS: [TranslationModelInfo; 1] = [TranslationModelInfo {
    name: DEFAULT_TRANSLATION_MODEL,
    repo: "JustFrederik/nllb-200-distilled-600M-ct2-int8",
    required_files: &[
        "config.json",
        "model.bin",
        "shared_vocabulary.json",
        "tokenizer.json",
    ],
}];

pub fn models_dir() -> PathBuf {
    let data_dir = dirs::data_dir().expect("Could not determine data directory");
    data_dir.join("com.wren.app").join("models").join("nllb")
}

pub fn model_path(model_name: &str) -> PathBuf {
    models_dir().join(model_name)
}

pub fn model_exists(model_name: &str) -> bool {
    find_model(model_name)
        .map(|model| {
            let path = model_path(model.name);
            model
                .required_files
                .iter()
                .all(|file| path.join(file).exists())
        })
        .unwrap_or(false)
}

fn find_model(model_name: &str) -> Option<&'static TranslationModelInfo> {
    AVAILABLE_TRANSLATION_MODELS
        .iter()
        .find(|m| m.name == model_name)
}

fn resolve_url(repo: &str, filename: &str) -> String {
    format!("{HF_BASE_URL}/{repo}/resolve/main/{filename}?download=true")
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub async fn download_model<F>(model_name: &str, progress_callback: F) -> Result<PathBuf>
where
    F: Fn(u64, u64),
{
    let model = find_model(model_name)
        .ok_or_else(|| anyhow!("Unknown translation model: {}", model_name))?;

    let model_dir = model_path(model_name);
    std::fs::create_dir_all(&model_dir)?;

    let client = reqwest::Client::new();
    let mut downloaded_total: u64 = 0;
    let mut expected_total: u64 = 0;

    for filename in model.required_files {
        let dest = model_dir.join(filename);
        if dest.exists() {
            continue;
        }

        let url = resolve_url(model.repo, filename);
        let response = client.get(&url).send().await?.error_for_status()?;
        let content_len = response.content_length().unwrap_or(0);
        expected_total = expected_total.saturating_add(content_len);

        let tmp = dest.with_extension("part");
        ensure_parent(&tmp)?;
        let mut file = std::fs::File::create(&tmp)?;
        let mut stream = response.bytes_stream();

        use std::io::Write;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk)?;
            downloaded_total = downloaded_total.saturating_add(chunk.len() as u64);
            progress_callback(downloaded_total, expected_total);
        }

        std::fs::rename(tmp, dest)?;
    }

    Ok(model_dir)
}
