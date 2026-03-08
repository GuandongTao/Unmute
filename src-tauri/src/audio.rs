use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{WavSpec, WavWriter};
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Wrapper to make cpal::Stream storable across threads.
/// Safety: we only create/drop the stream from the main thread context,
/// and access is serialized through a Mutex.
struct StreamWrapper(cpal::Stream);
unsafe impl Send for StreamWrapper {}

/// Shared audio state.
pub struct AudioState {
    samples: Arc<Mutex<Vec<f32>>>,
    is_recording: Arc<Mutex<bool>>,
    sample_rate: Arc<Mutex<u32>>,
    active_stream: Mutex<Option<StreamWrapper>>,
}

impl AudioState {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            is_recording: Arc::new(Mutex::new(false)),
            sample_rate: Arc::new(Mutex::new(48000)),
            active_stream: Mutex::new(None),
        }
    }

    pub fn set_recording(&self, val: bool) {
        *self.is_recording.lock().unwrap() = val;
    }

    /// Start recording from the default input device.
    /// Any previously active stream is dropped first.
    pub fn start(&self) -> Result<(), String> {
        // Drop any existing stream first
        self.stop_stream();

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device found")?;

        log::info!("Recording from: {}", device.name().unwrap_or_default());

        let supported_config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get input config: {}", e))?;

        let native_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels();

        log::info!("Input: {}Hz, {} channels", native_rate, channels);

        self.samples.lock().unwrap().clear();
        *self.sample_rate.lock().unwrap() = native_rate;
        self.set_recording(true);

        let samples = self.samples.clone();
        let is_recording = self.is_recording.clone();

        let stream = device
            .build_input_stream(
                &supported_config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !*is_recording.lock().unwrap() {
                        return;
                    }
                    let mut buf = samples.lock().unwrap();
                    if channels > 1 {
                        for chunk in data.chunks(channels as usize) {
                            let sum: f32 = chunk.iter().sum();
                            buf.push(sum / channels as f32);
                        }
                    } else {
                        buf.extend_from_slice(data);
                    }
                },
                |err| {
                    log::error!("Audio stream error: {}", err);
                },
                None,
            )
            .map_err(|e| format!("Failed to build input stream: {}", e))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start stream: {}", e))?;

        *self.active_stream.lock().unwrap() = Some(StreamWrapper(stream));

        Ok(())
    }

    /// Drop the active stream, stopping audio capture.
    fn stop_stream(&self) {
        let mut stream = self.active_stream.lock().unwrap();
        if stream.is_some() {
            *stream = None; // Drop the stream
            log::info!("Audio stream stopped");
        }
    }

    /// Stop recording, drop the stream, and save captured audio to WAV.
    pub fn stop_and_save(&self) -> Result<PathBuf, String> {
        self.set_recording(false);
        self.stop_stream();

        let samples = self.samples.lock().unwrap().clone();
        if samples.is_empty() {
            return Err("No audio recorded".to_string());
        }

        let native_rate = *self.sample_rate.lock().unwrap();
        let target_rate = 16000u32;

        let resampled = if native_rate != target_rate {
            resample(&samples, native_rate, target_rate)
        } else {
            samples
        };

        let temp_dir = std::env::temp_dir().join("unmute");
        std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
        let wav_path = temp_dir.join(format!("{}.wav", uuid::Uuid::new_v4()));

        let spec = WavSpec {
            channels: 1,
            sample_rate: target_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let file = std::fs::File::create(&wav_path).map_err(|e| e.to_string())?;
        let buf_writer = BufWriter::new(file);
        let mut writer = WavWriter::new(buf_writer, spec).map_err(|e| e.to_string())?;

        for sample in &resampled {
            let s = (*sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
            writer.write_sample(s).map_err(|e| e.to_string())?;
        }
        writer.finalize().map_err(|e| e.to_string())?;

        let duration_secs = resampled.len() as f32 / target_rate as f32;
        log::info!("Saved {:.1}s audio to {:?}", duration_secs, wav_path);

        Ok(wav_path)
    }
}

fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (samples.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 * ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;

        let s = if idx + 1 < samples.len() {
            samples[idx] as f64 * (1.0 - frac) + samples[idx + 1] as f64 * frac
        } else {
            samples[idx.min(samples.len() - 1)] as f64
        };
        output.push(s as f32);
    }
    output
}
