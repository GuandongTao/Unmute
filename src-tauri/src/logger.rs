use chrono::Local;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct TranscriptionLog {
    pub timestamp: String,
    pub audio_duration_secs: f32,
    pub asr_model: String,
    pub asr_latency_ms: u64,
    pub raw_transcript: String,
    pub cleanup_mode: String,
    pub cleanup_model: Option<String>,
    pub cleanup_latency_ms: Option<u64>,
    pub cleaned_transcript: Option<String>,
    pub final_text: String,
    pub error: Option<String>,
}

pub fn log_dir() -> PathBuf {
    let dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("unmute")
        .join("logs");
    fs::create_dir_all(&dir).ok();
    dir
}

pub fn write_log(entry: &TranscriptionLog) {
    let path = log_dir().join(format!("{}.jsonl", Local::now().format("%Y-%m-%d")));

    let line = match serde_json::to_string(entry) {
        Ok(json) => json,
        Err(e) => {
            log::error!("Failed to serialize log entry: {}", e);
            return;
        }
    };

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path);

    match file {
        Ok(mut f) => {
            if let Err(e) = writeln!(f, "{}", line) {
                log::error!("Failed to write log: {}", e);
            }
        }
        Err(e) => log::error!("Failed to open log file {:?}: {}", path, e),
    }
}
