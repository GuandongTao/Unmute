use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Remove whisper special tokens like <|endoftext|>, <|startoftranscript|>, etc.
fn strip_special_tokens(text: &str) -> String {
    let mut result = text.to_string();
    // Remove all <|...|> tokens
    while let Some(start) = result.find("<|") {
        if let Some(end) = result[start..].find("|>") {
            result.replace_range(start..start + end + 2, "");
        } else {
            break;
        }
    }
    result.trim().to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
    pub duration_ms: u64,
}

pub struct WhisperEngine {
    binary_path: String,
    model_path: String,
    language: String,
}

impl WhisperEngine {
    pub fn new(binary_path: &str, model_path: &str, language: &str) -> Self {
        Self {
            binary_path: binary_path.to_string(),
            model_path: model_path.to_string(),
            language: language.to_string(),
        }
    }

    /// Resolve the whisper binary path.
    /// If user configured a path, use it. Otherwise look for whisper-cli in PATH.
    pub fn resolve_binary(configured_path: &str) -> Result<String, String> {
        if !configured_path.is_empty() && Path::new(configured_path).exists() {
            return Ok(configured_path.to_string());
        }

        // Check common names in PATH
        for name in &["whisper-cli", "whisper-cpp", "main"] {
            if let Ok(output) = Command::new("where").arg(name).output() {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if !path.is_empty() {
                        return Ok(path);
                    }
                }
            }
        }

        Err("whisper-cli not found. Install whisper.cpp and add to PATH, or set whisper_path in config.".to_string())
    }

    /// Resolve the model file path.
    /// Looks in models_dir for the expected ggml model file.
    pub fn resolve_model(models_dir: &str, model_name: &str) -> Result<String, String> {
        let filename = format!("ggml-{}.bin", model_name);

        // Check configured models dir
        if !models_dir.is_empty() {
            let path = Path::new(models_dir).join(&filename);
            if path.exists() {
                return Ok(path.to_string_lossy().to_string());
            }
        }

        // Check default locations
        let default_dirs = vec![
            dirs::data_dir()
                .unwrap_or_default()
                .join("unmute")
                .join("models"),
            dirs::home_dir()
                .unwrap_or_default()
                .join(".unmute")
                .join("models"),
        ];

        for dir in default_dirs {
            let path = dir.join(&filename);
            if path.exists() {
                return Ok(path.to_string_lossy().to_string());
            }
        }

        Err(format!(
            "Model '{}' not found. Download it and place in models directory.\n\
             Expected file: {}\n\
             Download from: https://huggingface.co/ggerganov/whisper.cpp/tree/main",
            model_name, filename
        ))
    }

    pub fn transcribe(&self, audio_path: &str) -> Result<Transcript, String> {
        let start = std::time::Instant::now();

        if !Path::new(&self.binary_path).exists() && !self.binary_path.contains('/') && !self.binary_path.contains('\\') {
            // It might be in PATH, try using it directly
        } else if !Path::new(&self.binary_path).exists() {
            return Err(format!("Whisper binary not found: {}", self.binary_path));
        }

        if !Path::new(&self.model_path).exists() {
            return Err(format!("Whisper model not found: {}", self.model_path));
        }

        log::info!("Transcribing {:?} with model {}", audio_path, self.model_path);

        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("-m").arg(&self.model_path)
            .arg("-f").arg(audio_path)
            .arg("--no-timestamps")
            .arg("-l").arg(&self.language)
            .arg("--no-prints");

        let output = cmd.output().map_err(|e| format!("Failed to run whisper: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Whisper failed: {}", stderr));
        }

        let raw_text = String::from_utf8_lossy(&output.stdout).to_string();
        // Strip whisper special tokens like <|endoftext|>, <|startoftime|>, etc.
        let text = strip_special_tokens(raw_text.trim());

        let duration_ms = start.elapsed().as_millis() as u64;
        log::info!("ASR completed in {}ms: {:?}", duration_ms, text);

        Ok(Transcript {
            text,
            language: Some(self.language.clone()),
            duration_ms,
        })
    }
}
