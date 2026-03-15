use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

const WHISPER_VERSION: &str = "v1.8.3";
const HF_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

fn github_url(file: &str) -> String {
    format!(
        "https://github.com/ggml-org/whisper.cpp/releases/download/{}/{}",
        WHISPER_VERSION, file
    )
}

pub fn app_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_default()
        .join("unmute")
}

pub fn bin_dir() -> PathBuf {
    app_dir().join("bin")
}

pub fn gpu_bin_dir() -> PathBuf {
    app_dir().join("bin-gpu")
}

pub fn models_dir() -> PathBuf {
    app_dir().join("models")
}

#[derive(serde::Serialize, Clone)]
pub struct SetupStatus {
    pub has_whisper_cpu: bool,
    pub has_whisper_gpu: bool,
    pub has_model: bool,
    pub model_name: String,
    pub needs_setup: bool,
}

pub fn check(model: &str) -> SetupStatus {
    let has_cpu = bin_dir().join("whisper-cli.exe").exists();
    let has_gpu = gpu_bin_dir().join("whisper-cli.exe").exists();
    let model_file = format!("ggml-{}.bin", model);
    let has_model = models_dir().join(&model_file).exists();

    SetupStatus {
        has_whisper_cpu: has_cpu,
        has_whisper_gpu: has_gpu,
        has_model,
        model_name: model.to_string(),
        needs_setup: !has_cpu || !has_model,
    }
}

async fn download_with_progress(
    url: &str,
    dest: &Path,
    app: &AppHandle,
    step: &str,
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let total = resp.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;

    let mut last_emit = std::time::Instant::now();

    // Use resp.chunk() which doesn't require futures-util
    let mut resp = resp;
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("Download error: {}", e))? {
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        if last_emit.elapsed().as_millis() > 100 {
            app.emit(
                "setup-progress",
                serde_json::json!({
                    "step": step,
                    "downloaded": downloaded,
                    "total": total,
                }),
            )
            .ok();
            last_emit = std::time::Instant::now();
        }
    }

    // Final progress
    app.emit(
        "setup-progress",
        serde_json::json!({
            "step": step,
            "downloaded": downloaded,
            "total": total,
        }),
    )
    .ok();

    Ok(())
}

fn extract_whisper_zip(zip_path: &Path, dest_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;

    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();

        // Extract .exe and .dll files, flattened into dest_dir
        if let Some(filename) = name.split('/').last().or_else(|| name.split('\\').last()) {
            if filename.ends_with(".exe") || filename.ends_with(".dll") {
                let out_path = dest_dir.join(filename);
                let mut out_file = fs::File::create(&out_path).map_err(|e| e.to_string())?;
                std::io::copy(&mut entry, &mut out_file).map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}

pub async fn run(app: &AppHandle, model: &str, include_cuda: bool) -> Result<(), String> {
    let temp = std::env::temp_dir();

    // Step 1: whisper.cpp CPU
    if !bin_dir().join("whisper-cli.exe").exists() {
        let zip_path = temp.join("unmute-whisper-cpu.zip");
        let url = github_url("whisper-bin-x64.zip");

        app.emit("setup-status", "Downloading whisper.cpp (CPU)...")
            .ok();
        download_with_progress(&url, &zip_path, app, "whisper-cpu").await?;

        app.emit("setup-status", "Extracting whisper.cpp (CPU)...")
            .ok();
        extract_whisper_zip(&zip_path, &bin_dir())?;
        fs::remove_file(&zip_path).ok();
    }

    // Step 2: whisper.cpp CUDA (optional)
    if include_cuda && !gpu_bin_dir().join("whisper-cli.exe").exists() {
        let zip_path = temp.join("unmute-whisper-cuda.zip");
        let url = github_url("whisper-cublas-12.4.0-bin-x64.zip");

        app.emit("setup-status", "Downloading whisper.cpp (CUDA)...")
            .ok();
        download_with_progress(&url, &zip_path, app, "whisper-cuda").await?;

        app.emit("setup-status", "Extracting whisper.cpp (CUDA)...")
            .ok();
        extract_whisper_zip(&zip_path, &gpu_bin_dir())?;
        fs::remove_file(&zip_path).ok();
    }

    // Step 3: ASR model
    let model_file = format!("ggml-{}.bin", model);
    let model_path = models_dir().join(&model_file);
    if !model_path.exists() {
        let url = format!("{}/{}", HF_BASE, model_file);

        app.emit(
            "setup-status",
            format!("Downloading model ({})...", model),
        )
        .ok();
        download_with_progress(&url, &model_path, app, "model").await?;
    }

    app.emit("setup-status", "Setup complete!").ok();
    Ok(())
}
