use std::path::PathBuf;

pub const DEFAULT_TRANSLATION_MODEL: &str = "nllb-200-distilled-600M-int8";

pub fn models_dir() -> PathBuf {
    let data_dir = dirs::data_dir().expect("Could not determine data directory");
    data_dir.join("com.wren.app").join("models").join("nllb")
}

pub fn model_path(model_name: &str) -> PathBuf {
    models_dir().join(model_name)
}

pub fn model_exists(model_name: &str) -> bool {
    model_path(model_name).exists()
}
