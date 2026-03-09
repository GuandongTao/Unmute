use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

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
    pub device_used: String,
}

pub struct WhisperEngine {
    binary_path: String,
    fallback_binary_path: Option<String>,
    model_path: String,
    language: String,
    translate: bool,
}

impl WhisperEngine {
    pub fn new(binary_path: &str, model_path: &str, language: &str, fallback_binary_path: Option<&str>, translate: bool) -> Self {
        Self {
            binary_path: binary_path.to_string(),
            fallback_binary_path: fallback_binary_path.map(|s| s.to_string()),
            model_path: model_path.to_string(),
            language: language.to_string(),
            translate,
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
            let mut cmd = Command::new("where");
            cmd.arg(name);
            #[cfg(target_os = "windows")]
            cmd.creation_flags(CREATE_NO_WINDOW);
            if let Ok(output) = cmd.output() {
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

    fn run_whisper(&self, binary_path: &str, audio_path: &str) -> Result<(String, u64), String> {
        let start = std::time::Instant::now();

        if !Path::new(binary_path).exists() && binary_path.contains('\\') || binary_path.contains('/') {
            return Err(format!("Whisper binary not found: {}", binary_path));
        }

        let mut cmd = Command::new(binary_path);
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.arg("-m").arg(&self.model_path)
            .arg("-f").arg(audio_path)
            .arg("--no-timestamps")
            .arg("-l").arg(&self.language)
            .arg("--no-prints");

        if self.translate {
            cmd.arg("--translate");
        }

        let output = cmd.output().map_err(|e| format!("Failed to run whisper: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Whisper failed: {}", stderr));
        }

        let raw_text = String::from_utf8_lossy(&output.stdout).to_string();
        let text = strip_special_tokens(raw_text.trim());
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok((text, duration_ms))
    }

    pub fn transcribe(&self, audio_path: &str, device_label: &str) -> Result<Transcript, String> {
        if !Path::new(&self.model_path).exists() {
            return Err(format!("Whisper model not found: {}", self.model_path));
        }

        log::info!("Transcribing {:?} with model {} ({})", audio_path, self.model_path, device_label);

        // Try primary binary
        match self.run_whisper(&self.binary_path, audio_path) {
            Ok((text, duration_ms)) => {
                log::info!("ASR completed in {}ms ({}): {:?}", duration_ms, device_label, text);
                return Ok(Transcript {
                    text,
                    language: Some(self.language.clone()),
                    duration_ms,
                    device_used: device_label.to_string(),
                });
            }
            Err(e) => {
                // If we have a fallback, try it
                if let Some(ref fallback) = self.fallback_binary_path {
                    log::warn!("{} ASR failed ({}), falling back to CPU", device_label, e);
                    match self.run_whisper(fallback, audio_path) {
                        Ok((text, duration_ms)) => {
                            log::info!("ASR completed in {}ms (CPU fallback): {:?}", duration_ms, text);
                            return Ok(Transcript {
                                text,
                                language: Some(self.language.clone()),
                                duration_ms,
                                device_used: "CPU*".to_string(),
                            });
                        }
                        Err(e2) => {
                            return Err(format!("ASR failed on both GPU and CPU.\nGPU: {}\nCPU: {}", e, e2));
                        }
                    }
                }
                return Err(e);
            }
        }
    }
}
