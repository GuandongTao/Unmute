use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub asr_model: String,
    pub asr_language: String,
    pub asr_device: AsrDevice,
    pub whisper_path: String,
    pub whisper_gpu_path: String,
    pub models_dir: String,
    pub cleanup_mode: CleanupMode,
    pub cleanup_device: CleanupDevice,
    pub cleanup_model: String,
    pub ollama_url: String,
    pub auto_paste: bool,
    pub max_recording_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AsrDevice {
    Cpu,
    Gpu,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CleanupMode {
    Off,
    Light,
    Rewrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CleanupDevice {
    Gpu,
    Cpu,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            asr_model: "small.en".to_string(),
            asr_language: "en".to_string(),
            asr_device: AsrDevice::Cpu,
            whisper_path: String::new(),
            whisper_gpu_path: String::new(),
            models_dir: String::new(),
            cleanup_mode: CleanupMode::Off,
            cleanup_device: CleanupDevice::Gpu,
            cleanup_model: "qwen3.5:0.8b".to_string(),
            ollama_url: "http://localhost:11434".to_string(),
            auto_paste: true,
            max_recording_secs: 120,
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("unmute");
        fs::create_dir_all(&config_dir).ok();
        config_dir.join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            let content = fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            let config = Self::default();
            config.save().ok();
            config
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, content).map_err(|e| e.to_string())
    }
}
