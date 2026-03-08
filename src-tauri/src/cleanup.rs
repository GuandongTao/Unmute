use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::CleanupMode;

// --- Preserve original language prompts ---

const LIGHT_SYSTEM_PROMPT: &str = "\
You are a transcript cleanup assistant.\n\
Clean the text lightly.\n\
Remove filler words and false starts.\n\
Fix punctuation and grammar.\n\
Preserve wording and meaning.\n\
Keep English and Chinese exactly as spoken.\n\
Do not translate.\n\
Do not add information.\n\
Output only the cleaned text.";

const REWRITE_SYSTEM_PROMPT: &str = "\
You are a transcript-to-prose assistant.\n\
Clean up the transcript into clear written text.\n\
Remove filler words, repetition, and broken fragments.\n\
Only rewrite sentence structure if necessary for clarity.\n\
If the original phrasing is already clear, keep it as-is.\n\
Preserve meaning exactly.\n\
Keep English and Chinese mixed usage exactly as spoken.\n\
Do not translate.\n\
Do not add new information.\n\
Output only the cleaned text.";

// --- Translate-to-English prompts ---

const LIGHT_TRANSLATE_PROMPT: &str = "\
You are a transcript cleanup assistant.\n\
Clean the text lightly.\n\
Remove filler words and false starts.\n\
Fix punctuation and grammar.\n\
Translate all non-English text to English.\n\
Preserve meaning.\n\
Do not add information.\n\
Output only the cleaned English text.";

const REWRITE_TRANSLATE_PROMPT: &str = "\
You are a transcript-to-prose assistant.\n\
Clean up the transcript into clear written English text.\n\
Remove filler words, repetition, and broken fragments.\n\
Only rewrite sentence structure if necessary for clarity.\n\
If the original phrasing is already clear, keep it as-is.\n\
Translate all non-English text to English.\n\
Preserve meaning exactly.\n\
Do not add new information.\n\
Output only the cleaned English text.";

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    system: String,
    prompt: String,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

pub struct CleanupEngine {
    client: Client,
    ollama_url: String,
    model: String,
}

impl CleanupEngine {
    pub fn new(ollama_url: &str, model: &str) -> Self {
        Self {
            client: Client::new(),
            ollama_url: ollama_url.to_string(),
            model: model.to_string(),
        }
    }

    pub async fn cleanup(&self, text: &str, mode: &CleanupMode, translate_to_english: bool) -> Result<String, String> {
        if *mode == CleanupMode::Off {
            return Ok(text.to_string());
        }

        // Safety: skip cleanup for very short or empty text
        let word_count = text.split_whitespace().count();
        if word_count < 3 {
            log::info!("Text too short for cleanup ({} words), returning as-is", word_count);
            return Ok(text.to_string());
        }

        // Rewrite always translates to English; Light respects the toggle
        let system_prompt = match (mode, translate_to_english) {
            (CleanupMode::Light, false) => LIGHT_SYSTEM_PROMPT,
            (CleanupMode::Light, true) => LIGHT_TRANSLATE_PROMPT,
            (CleanupMode::Rewrite, _) => REWRITE_TRANSLATE_PROMPT,
            (CleanupMode::Off, _) => unreachable!(),
        };

        let start = std::time::Instant::now();

        let request = OllamaRequest {
            model: self.model.clone(),
            system: system_prompt.to_string(),
            prompt: text.to_string(),
            stream: false,
        };

        let url = format!("{}/api/generate", self.ollama_url);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Ollama request failed: {}. Is Ollama running?", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {}: {}", status, body));
        }

        let result: OllamaResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

        let cleaned = result.response.trim().to_string();
        let duration_ms = start.elapsed().as_millis();

        log::info!("Cleanup ({:?}) completed in {}ms", mode, duration_ms);
        log::info!("Raw: {:?}", text);
        log::info!("Cleaned: {:?}", cleaned);

        Ok(cleaned)
    }
}
