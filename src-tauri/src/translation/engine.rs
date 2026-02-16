use std::sync::mpsc;

#[derive(Debug, Clone)]
pub struct TranslationJob {
    pub text: String,
    pub source_lang: String,
    pub target_lang: String,
}

struct TranslationService {
    source_lang: Option<String>,
    target_lang: String,
    model_loaded: bool,
}

impl TranslationService {
    fn new() -> Self {
        Self {
            source_lang: Some("en".to_string()),
            target_lang: "en".to_string(),
            model_loaded: false,
        }
    }

    fn load_model(&mut self, _path: Option<String>) -> Result<(), String> {
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

        if job.source_lang == job.target_lang {
            return Ok(text.to_string());
        }

        // Placeholder backend for pipeline integration. The concrete model
        // implementation will replace this passthrough behavior.
        Ok(text.to_string())
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
