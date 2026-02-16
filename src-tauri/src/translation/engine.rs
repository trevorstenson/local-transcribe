use std::sync::mpsc;

use ct2rs::sys::ComputeType;
use ct2rs::{Config, TranslationOptions, Translator};
use whatlang::{detect, Lang};

#[derive(Debug, Clone)]
pub struct TranslationJob {
    pub text: String,
    pub source_lang: String,
    pub target_lang: String,
}

struct TranslationService {
    translator: Option<Translator<ct2rs::tokenizers::auto::Tokenizer>>,
    source_lang: Option<String>,
    target_lang: String,
    model_loaded: bool,
}

impl TranslationService {
    fn new() -> Self {
        Self {
            translator: None,
            source_lang: Some("en".to_string()),
            target_lang: "en".to_string(),
            model_loaded: false,
        }
    }

    fn load_model(&mut self, path: Option<String>) -> Result<(), String> {
        let path = path.ok_or_else(|| "Missing translation model path".to_string())?;
        let mut config = Config::default();
        config.compute_type = ComputeType::INT8;
        config.num_threads_per_replica = std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4);

        let translator =
            Translator::new(&path, &config).map_err(|e| format!("Failed to load model: {}", e))?;
        self.translator = Some(translator);
        self.model_loaded = true;
        Ok(())
    }

    fn translate(&self, job: &TranslationJob) -> Result<String, String> {
        if !self.model_loaded {
            return Err("Translation model not loaded".to_string());
        }

        let text = job.text.trim();
        if text.is_empty() {
            return Ok(String::new());
        }

        let target_nllb = nllb_lang_for_app_lang(&job.target_lang)
            .ok_or_else(|| format!("Unsupported target language '{}'", job.target_lang))?;
        let source_nllb = resolve_source_nllb_lang(&job.source_lang, text)
            .ok_or_else(|| format!("Unsupported source language '{}'", job.source_lang))?;

        if source_nllb == target_nllb {
            return Ok(text.to_string());
        }

        let translator = self
            .translator
            .as_ref()
            .ok_or_else(|| "Translation model not initialized".to_string())?;

        // For the ct2rs NLLB path, keep source as plain text and drive translation
        // direction via target prefix language token.
        let sources = vec![text.to_string()];
        let target_prefixes = vec![vec![target_nllb.to_string()]];

        let mut options = TranslationOptions::<String, String>::default();
        options.beam_size = 1;
        options.max_decoding_length = 256;

        let output = translator
            .translate_batch_with_target_prefix(&sources, &target_prefixes, &options, None)
            .map_err(|e| format!("Translation inference failed: {}", e))?;

        let translated = output
            .into_iter()
            .next()
            .map(|(text, _)| text.trim().to_string())
            .unwrap_or_default();

        if translated.is_empty() {
            Ok(text.to_string())
        } else {
            Ok(translated)
        }
    }
}

fn resolve_source_nllb_lang(source_lang: &str, text: &str) -> Option<&'static str> {
    if source_lang == "auto" {
        detect(text)
            .and_then(|info| nllb_lang_for_detected(info.lang()))
            .or(Some("eng_Latn"))
    } else {
        nllb_lang_for_app_lang(source_lang)
    }
}

fn nllb_lang_for_detected(lang: Lang) -> Option<&'static str> {
    match lang {
        Lang::Eng => Some("eng_Latn"),
        Lang::Spa => Some("spa_Latn"),
        Lang::Fra => Some("fra_Latn"),
        Lang::Deu => Some("deu_Latn"),
        Lang::Ita => Some("ita_Latn"),
        Lang::Por => Some("por_Latn"),
        Lang::Cmn => Some("zho_Hans"),
        Lang::Jpn => Some("jpn_Jpan"),
        Lang::Kor => Some("kor_Hang"),
        Lang::Rus => Some("rus_Cyrl"),
        Lang::Ara => Some("arb_Arab"),
        Lang::Hin => Some("hin_Deva"),
        Lang::Nld => Some("nld_Latn"),
        Lang::Pol => Some("pol_Latn"),
        Lang::Tur => Some("tur_Latn"),
        Lang::Swe => Some("swe_Latn"),
        Lang::Ukr => Some("ukr_Cyrl"),
        _ => None,
    }
}

fn nllb_lang_for_app_lang(lang: &str) -> Option<&'static str> {
    match lang {
        "en" => Some("eng_Latn"),
        "es" => Some("spa_Latn"),
        "fr" => Some("fra_Latn"),
        "de" => Some("deu_Latn"),
        "it" => Some("ita_Latn"),
        "pt" => Some("por_Latn"),
        "zh" => Some("zho_Hans"),
        "ja" => Some("jpn_Jpan"),
        "ko" => Some("kor_Hang"),
        "ru" => Some("rus_Cyrl"),
        "ar" => Some("arb_Arab"),
        "hi" => Some("hin_Deva"),
        "nl" => Some("nld_Latn"),
        "pl" => Some("pol_Latn"),
        "tr" => Some("tur_Latn"),
        "sv" => Some("swe_Latn"),
        "uk" => Some("ukr_Cyrl"),
        _ => None,
    }
}

pub enum TranslationRequest {
    LoadModel(Option<String>),
    SetLanguages {
        source: Option<String>,
        target: String,
    },
    Translate(TranslationJob),
    TranslatePartial(TranslationJob),
    Shutdown,
}

pub enum TranslationResponse {
    ModelLoaded(Result<(), String>),
    TranslationComplete(Result<String, String>),
}

pub fn spawn_translation_thread() -> (
    mpsc::Sender<TranslationRequest>,
    mpsc::Receiver<TranslationResponse>,
    mpsc::Receiver<String>,
) {
    let (req_tx, req_rx) = mpsc::channel::<TranslationRequest>();
    let (resp_tx, resp_rx) = mpsc::channel::<TranslationResponse>();
    let (partial_tx, partial_rx) = mpsc::channel::<String>();

    std::thread::spawn(move || {
        let mut service = TranslationService::new();

        while let Ok(request) = req_rx.recv() {
            match request {
                TranslationRequest::LoadModel(path) => {
                    let result = service.load_model(path);
                    let _ = resp_tx.send(TranslationResponse::ModelLoaded(result));
                }
                TranslationRequest::SetLanguages { source, target } => {
                    service.source_lang = source;
                    service.target_lang = target;
                }
                TranslationRequest::Translate(job) => {
                    let result = service.translate(&job);
                    let _ = resp_tx.send(TranslationResponse::TranslationComplete(result));
                }
                TranslationRequest::TranslatePartial(job) => {
                    // Drain stale partials — only process the newest one.
                    let mut latest_job = job;
                    let mut got_final = None;
                    while let Ok(queued) = req_rx.try_recv() {
                        match queued {
                            TranslationRequest::TranslatePartial(newer) => {
                                latest_job = newer;
                            }
                            TranslationRequest::Translate(final_job) => {
                                got_final = Some(final_job);
                                break;
                            }
                            TranslationRequest::LoadModel(path) => {
                                let result = service.load_model(path);
                                let _ = resp_tx.send(TranslationResponse::ModelLoaded(result));
                            }
                            TranslationRequest::SetLanguages { source, target } => {
                                service.source_lang = source;
                                service.target_lang = target;
                            }
                            TranslationRequest::Shutdown => {
                                return;
                            }
                        }
                    }

                    if let Some(final_job) = got_final {
                        let result = service.translate(&final_job);
                        let _ = resp_tx.send(TranslationResponse::TranslationComplete(result));
                    } else {
                        let result = service.translate(&latest_job);
                        if let Ok(text) = result {
                            let _ = partial_tx.send(text.trim().to_string());
                        }
                    }
                }
                TranslationRequest::Shutdown => {
                    break;
                }
            }
        }
    });

    (req_tx, resp_rx, partial_rx)
}
