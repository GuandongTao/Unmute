// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod asr;
mod audio;
mod cleanup;
mod config;
mod hotkey;
mod logger;
mod paste;

use audio::AudioState;
use config::{CleanupMode, Config};
use hotkey::HotkeyEvent;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WebviewWindowBuilder,
};

/// Shared app state — only contains Send+Sync types
struct AppState {
    audio: AudioState,
    config: Mutex<Config>,
    is_recording: Mutex<bool>,
    is_toggle_mode: Mutex<bool>,
}

// Safety: AudioState uses Arc<Mutex<_>> internally, all fields are Send+Sync.
// The cpal::Stream (which is !Send) is NOT stored here.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

#[tauri::command]
fn get_config(state: tauri::State<'_, Arc<AppState>>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
fn update_config(
    state: tauri::State<'_, Arc<AppState>>,
    new_config: Config,
) -> Result<(), String> {
    new_config.save()?;
    *state.config.lock().unwrap() = new_config;
    Ok(())
}

#[tauri::command]
fn get_status(state: tauri::State<'_, Arc<AppState>>) -> String {
    if *state.is_recording.lock().unwrap() {
        "recording".to_string()
    } else {
        "idle".to_string()
    }
}

/// List available whisper models found in the models directory.
#[tauri::command]
fn list_models(state: tauri::State<'_, Arc<AppState>>) -> Vec<String> {
    let config = state.config.lock().unwrap();
    let mut models = Vec::new();

    let dirs_to_check = vec![
        config.models_dir.clone(),
        dirs::data_dir()
            .unwrap_or_default()
            .join("unmute")
            .join("models")
            .to_string_lossy()
            .to_string(),
    ];

    for dir in dirs_to_check {
        if dir.is_empty() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("ggml-") && name.ends_with(".bin") {
                    let model = name
                        .strip_prefix("ggml-")
                        .unwrap_or(&name)
                        .strip_suffix(".bin")
                        .unwrap_or(&name)
                        .to_string();
                    if !models.contains(&model) {
                        models.push(model);
                    }
                }
            }
        }
    }

    models.sort();
    models
}

/// List available Ollama models by querying the local Ollama API.
#[tauri::command]
async fn list_ollama_models(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    let config = state.config.lock().unwrap().clone();
    let url = format!("{}/api/tags", config.ollama_url);

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .map_err(|e| format!("Ollama not reachable: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Ollama returned {}", resp.status()));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

    let mut models = Vec::new();
    if let Some(arr) = body.get("models").and_then(|v| v.as_array()) {
        for m in arr {
            if let Some(name) = m.get("name").and_then(|v| v.as_str()) {
                models.push(name.to_string());
            }
        }
    }

    models.sort();
    Ok(models)
}

/// Update the overlay pill color by directly evaluating JS in the overlay webview.
fn set_overlay_status(app: &AppHandle, status: &str) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let js = match status {
            "recording" => "document.getElementById('pill').className = 'recording'; \
                            document.getElementById('timing').textContent = '';".to_string(),
            "processing" => "document.getElementById('pill').className = 'processing'; \
                             document.getElementById('timing').textContent = '';".to_string(),
            "error" => "document.getElementById('pill').className = 'error'; \
                        document.getElementById('timing').textContent = ''; \
                        setTimeout(() => document.getElementById('pill').className = '', 3000);".to_string(),
            _ => "document.getElementById('pill').className = ''; \
                  document.getElementById('timing').textContent = '';".to_string(),
        };
        overlay.eval(&js).ok();
    }
}

/// Show timing on the pill after processing completes, then fade back to idle.
fn set_overlay_done(app: &AppHandle, asr_device: &str, asr_ms: u64, cleanup_ms: Option<u64>) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let asr_str = format!("{:.1}s", asr_ms as f64 / 1000.0);
        let llm_str = match cleanup_ms {
            Some(ms) => format!("{:.1}s", ms as f64 / 1000.0),
            None => "\u{2014}".to_string(), // em dash
        };
        let label = format!("{} ASR {} | LLM {}", asr_device, asr_str, llm_str);
        let js = format!(
            "document.getElementById('pill').className = 'done'; \
             document.getElementById('timing').textContent = '{}'; \
             setTimeout(() => {{ \
               document.getElementById('pill').className = ''; \
               document.getElementById('timing').textContent = ''; \
             }}, 5000);",
            label
        );
        overlay.eval(&js).ok();
    }
}

fn start_recording(state: &Arc<AppState>, app: &AppHandle) {
    let mut is_recording = state.is_recording.lock().unwrap();
    if *is_recording {
        return;
    }

    match state.audio.start() {
        Ok(()) => {
            *is_recording = true;
            log::info!("Recording started");
            app.emit("recording-status", "recording").ok();
            set_overlay_status(app, "recording");
        }
        Err(e) => {
            log::error!("Failed to start recording: {}", e);
            app.emit("error", e).ok();
            set_overlay_status(app, "error");
        }
    }
}

fn stop_recording_and_process(state: Arc<AppState>, app: AppHandle) {
    {
        let mut is_recording = state.is_recording.lock().unwrap();
        if !*is_recording {
            return;
        }
        *is_recording = false;
        *state.is_toggle_mode.lock().unwrap() = false;
    }

    // Stop the audio capture (the stream callbacks will stop collecting)
    state.audio.set_recording(false);

    app.emit("recording-status", "processing").ok();
    set_overlay_status(&app, "processing");

    // Process in background
    tauri::async_runtime::spawn(async move {
        let result = process_recording(&state).await;

        match result {
            Ok(pr) => {
                app.emit("transcription-result", &pr).ok();
                app.emit("recording-status", "idle").ok();
                set_overlay_done(&app, &pr.asr_device, pr.asr_ms, pr.cleanup_ms);
            }
            Err(e) => {
                log::error!("Processing failed: {}", e);
                app.emit("error", &e).ok();
                set_overlay_status(&app, "error");
            }
        }
    });
}

#[derive(serde::Serialize, Clone)]
struct ProcessResult {
    text: String,
    asr_device: String,
    asr_ms: u64,
    cleanup_ms: Option<u64>,
}

async fn process_recording(state: &Arc<AppState>) -> Result<ProcessResult, String> {
    // Save recorded audio to WAV
    let wav_path = state.audio.stop_and_save()?;
    let wav_path_str = wav_path.to_string_lossy().to_string();
    let config = state.config.lock().unwrap().clone();

    // Run ASR — pick binary based on asr_device setting, with CPU fallback
    let model = asr::WhisperEngine::resolve_model(&config.models_dir, &config.asr_model)?;
    let (primary_binary, fallback_binary, device_label) = if config.asr_device == config::AsrDevice::Gpu && !config.whisper_gpu_path.is_empty() {
        let gpu = asr::WhisperEngine::resolve_binary(&config.whisper_gpu_path)?;
        let cpu = asr::WhisperEngine::resolve_binary(&config.whisper_path).ok();
        (gpu, cpu, "GPU")
    } else {
        let cpu = asr::WhisperEngine::resolve_binary(&config.whisper_path)?;
        (cpu, None, "CPU")
    };
    let engine = asr::WhisperEngine::new(&primary_binary, &model, &config.asr_language, fallback_binary.as_deref());

    let transcript = engine.transcribe(&wav_path_str, device_label)?;

    if transcript.text.is_empty() {
        std::fs::remove_file(&wav_path).ok();
        return Err("No speech detected".to_string());
    }

    let mut final_text = transcript.text.clone();
    let mut cleanup_latency = None;
    let mut cleaned = None;

    // Run cleanup if enabled
    if config.cleanup_mode != CleanupMode::Off {
        let cleanup_engine =
            cleanup::CleanupEngine::new(&config.ollama_url, &config.cleanup_model);

        let start = std::time::Instant::now();
        match cleanup_engine
            .cleanup(&transcript.text, &config.cleanup_mode)
            .await
        {
            Ok(cleaned_text) => {
                cleanup_latency = Some(start.elapsed().as_millis() as u64);
                cleaned = Some(cleaned_text.clone());
                final_text = cleaned_text;
            }
            Err(e) => {
                log::warn!("Cleanup failed, using raw transcript: {}", e);
            }
        }
    }

    // Paste final text (after cleanup if enabled)
    if config.auto_paste {
        paste::paste_text(&final_text)?;
    }

    // Calculate audio duration
    let audio_duration = if let Ok(reader) = hound::WavReader::open(&wav_path) {
        let spec = reader.spec();
        let samples = reader.len();
        samples as f32 / spec.sample_rate as f32
    } else {
        0.0
    };

    // Log
    logger::write_log(&logger::TranscriptionLog {
        timestamp: chrono::Local::now().to_rfc3339(),
        audio_duration_secs: audio_duration,
        asr_model: config.asr_model.clone(),
        asr_latency_ms: transcript.duration_ms,
        raw_transcript: transcript.text.clone(),
        cleanup_mode: format!("{:?}", config.cleanup_mode),
        cleanup_model: if config.cleanup_mode != CleanupMode::Off {
            Some(config.cleanup_model.clone())
        } else {
            None
        },
        cleanup_latency_ms: cleanup_latency,
        cleaned_transcript: cleaned,
        final_text: final_text.clone(),
        error: None,
    });

    std::fs::remove_file(&wav_path).ok();

    Ok(ProcessResult {
        text: final_text,
        asr_device: transcript.device_used.clone(),
        asr_ms: transcript.duration_ms,
        cleanup_ms: cleanup_latency,
    })
}

/// Ensure Ollama is running, start it if not.
fn ensure_ollama(config: &Config) {
    if config.cleanup_mode == CleanupMode::Off {
        return;
    }
    // Check if Ollama is reachable
    let url = format!("{}/api/tags", config.ollama_url);
    let reachable = reqwest::blocking::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .is_ok();

    if !reachable {
        log::info!("Ollama not running, attempting to start...");
        // Try common install locations
        let ollama_paths = vec![
            dirs::data_local_dir()
                .unwrap_or_default()
                .join("Programs")
                .join("Ollama")
                .join("ollama.exe"),
        ];
        for path in &ollama_paths {
            if path.exists() {
                match std::process::Command::new(path)
                    .arg("serve")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    Ok(_) => {
                        log::info!("Started Ollama from {:?}", path);
                        // Give it a moment to start
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        return;
                    }
                    Err(e) => {
                        log::warn!("Failed to start Ollama from {:?}: {}", path, e);
                    }
                }
            }
        }
        log::warn!("Could not start Ollama automatically");
    } else {
        log::info!("Ollama is already running");
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = Config::load();
    log::info!("Unmute starting with config: {:?}", config);

    ensure_ollama(&config);

    let state = Arc::new(AppState {
        audio: AudioState::new(),
        config: Mutex::new(config),
        is_recording: Mutex::new(false),
        is_toggle_mode: Mutex::new(false),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state.clone())
        .setup(move |app| {
            let handle = app.handle().clone();

            // Build system tray
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Unmute - Voice Typing")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        hotkey::remove_hook();
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            window.show().ok();
                            window.set_focus().ok();
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            // Create overlay indicator window
            let monitor = handle.primary_monitor()?.or_else(|| handle.available_monitors().ok()?.into_iter().next());
            let overlay_w = 200.0;
            let overlay_h = 28.0;
            let (overlay_x, overlay_y) = if let Some(m) = monitor {
                let pos = m.position();
                let size = m.size();
                let scale = m.scale_factor();
                // Center horizontally, just above taskbar (~48px from bottom)
                let x = pos.x as f64 + (size.width as f64 / 2.0) - (overlay_w * scale / 2.0);
                let y = pos.y as f64 + size.height as f64 - (60.0 * scale);
                (x / scale, y / scale)
            } else {
                (960.0 - overlay_w / 2.0, 1080.0 - 60.0)
            };

            let overlay = WebviewWindowBuilder::new(
                app,
                "overlay",
                tauri::WebviewUrl::App("overlay.html".into()),
            )
            .title("")
            .inner_size(overlay_w, overlay_h)
            .position(overlay_x, overlay_y)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .visible(true)
            .build()?;

            overlay.set_ignore_cursor_events(true)?;

            // Install low-level keyboard hook for hotkeys
            let state_for_hook = state.clone();
            let handle_for_hook = handle.clone();

            hotkey::install_hook(move |event| {
                let is_toggle = *state_for_hook.is_toggle_mode.lock().unwrap();
                match event {
                    HotkeyEvent::HoldStart => {
                        if is_toggle {
                            return; // Don't interfere with toggle mode
                        }
                        log::info!("Hold-to-talk: START");
                        start_recording(&state_for_hook, &handle_for_hook);
                    }
                    HotkeyEvent::HoldStop => {
                        if is_toggle {
                            return; // Don't interfere with toggle mode
                        }
                        log::info!("Hold-to-talk: STOP");
                        stop_recording_and_process(
                            state_for_hook.clone(),
                            handle_for_hook.clone(),
                        );
                    }
                    HotkeyEvent::TogglePressed => {
                        let is_recording = *state_for_hook.is_recording.lock().unwrap();
                        let is_toggle = *state_for_hook.is_toggle_mode.lock().unwrap();
                        if is_recording && is_toggle {
                            log::info!("Toggle-to-talk: STOP");
                            stop_recording_and_process(
                                state_for_hook.clone(),
                                handle_for_hook.clone(),
                            );
                        } else {
                            log::info!("Toggle-to-talk: START");
                            *state_for_hook.is_toggle_mode.lock().unwrap() = true;
                            start_recording(&state_for_hook, &handle_for_hook);

                            // Auto-stop timer
                            let max_secs = state_for_hook
                                .config
                                .lock()
                                .unwrap()
                                .max_recording_secs;
                            let st = state_for_hook.clone();
                            let hd = handle_for_hook.clone();
                            tauri::async_runtime::spawn(async move {
                                tokio::time::sleep(std::time::Duration::from_secs(max_secs))
                                    .await;
                                let is_toggle = *st.is_toggle_mode.lock().unwrap();
                                let is_rec = *st.is_recording.lock().unwrap();
                                if is_toggle && is_rec {
                                    log::info!("Auto-stopping after {}s max", max_secs);
                                    stop_recording_and_process(st, hd);
                                }
                            });
                        }
                    }
                }
            })
            .map_err(|e| tauri::Error::Anyhow(anyhow::anyhow!("{}", e)))?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_config, update_config, get_status, list_models, list_ollama_models])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
