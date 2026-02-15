use std::sync::mpsc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

struct TranscriptionService {
    context: Option<WhisperContext>,
}

impl TranscriptionService {
    fn new() -> Self {
        Self { context: None }
    }

    fn load_model(&mut self, path: &str) -> Result<(), String> {
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|e| format!("Failed to load Whisper model: {:?}", e))?;
        self.context = Some(ctx);
        Ok(())
    }

    fn transcribe(&self, audio_data: &[f32]) -> Result<String, String> {
        let ctx = self
            .context
            .as_ref()
            .ok_or_else(|| "Model not loaded".to_string())?;

        let mut state = ctx
            .create_state()
            .map_err(|e| format!("Failed to create whisper state: {:?}", e))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(4);
        params.set_language(Some("en"));
        params.set_no_context(true);
        params.set_single_segment(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_no_timestamps(true);
        params.set_print_progress(false);

        state
            .full(params, audio_data)
            .map_err(|e| format!("Transcription failed: {:?}", e))?;

        let mut text = String::new();
        for segment in state.as_iter() {
            if let Ok(s) = segment.to_str_lossy() {
                text.push_str(&s);
            }
        }

        Ok(text.trim().to_string())
    }
}

pub enum TranscriptionRequest {
    LoadModel(String),
    Transcribe(Vec<f32>),
    Shutdown,
}

pub enum TranscriptionResponse {
    ModelLoaded(Result<(), String>),
    TranscriptionComplete(Result<String, String>),
}

pub fn spawn_transcription_thread() -> (
    mpsc::Sender<TranscriptionRequest>,
    mpsc::Receiver<TranscriptionResponse>,
) {
    let (req_tx, req_rx) = mpsc::channel::<TranscriptionRequest>();
    let (resp_tx, resp_rx) = mpsc::channel::<TranscriptionResponse>();

    std::thread::spawn(move || {
        let mut service = TranscriptionService::new();

        while let Ok(request) = req_rx.recv() {
            match request {
                TranscriptionRequest::LoadModel(path) => {
                    let result = service.load_model(&path);
                    let _ = resp_tx.send(TranscriptionResponse::ModelLoaded(result));
                }
                TranscriptionRequest::Transcribe(audio_data) => {
                    let result = service.transcribe(&audio_data);
                    let _ = resp_tx.send(TranscriptionResponse::TranscriptionComplete(result));
                }
                TranscriptionRequest::Shutdown => {
                    break;
                }
            }
        }
    });

    (req_tx, resp_rx)
}
