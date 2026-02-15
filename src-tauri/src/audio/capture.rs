use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use std::sync::{Arc, Mutex};

use super::resampler;

pub struct AudioCapture {
    stream: Option<Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    device_sample_rate: u32,
}

impl AudioCapture {
    pub fn new() -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device available"))?;
        let config = device.default_input_config()?;
        let device_sample_rate = config.sample_rate();

        Ok(Self {
            stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            device_sample_rate,
        })
    }

    pub fn start_recording(&mut self) -> anyhow::Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device available"))?;
        let config = device.default_input_config()?;
        let channels = config.channels() as usize;
        let sample_format = config.sample_format();
        self.device_sample_rate = config.sample_rate();

        // Clear buffer before starting
        {
            let mut buf = self.buffer.lock().unwrap();
            buf.clear();
        }

        let buffer = Arc::clone(&self.buffer);

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mono: Vec<f32> = data
                        .chunks(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                        .collect();
                    let mut buf = buffer.lock().unwrap();
                    buf.extend_from_slice(&mono);
                },
                |err| eprintln!("Audio stream error: {}", err),
                None,
            )?,
            cpal::SampleFormat::I16 => {
                let buffer = Arc::clone(&self.buffer);
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let mono: Vec<f32> = data
                            .chunks(channels)
                            .map(|frame| {
                                let sum: f32 =
                                    frame.iter().map(|&s| s as f32 / i16::MAX as f32).sum();
                                sum / channels as f32
                            })
                            .collect();
                        let mut buf = buffer.lock().unwrap();
                        buf.extend_from_slice(&mono);
                    },
                    |err| eprintln!("Audio stream error: {}", err),
                    None,
                )?
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Unsupported sample format: {:?}",
                    sample_format
                ));
            }
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop_recording(&mut self) -> Vec<f32> {
        // Drop the stream to stop recording
        self.stream = None;

        let buffer = {
            let mut buf = self.buffer.lock().unwrap();
            std::mem::take(&mut *buf)
        };

        resampler::resample(&buffer, self.device_sample_rate, 16000)
    }
}
