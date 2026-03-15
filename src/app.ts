import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Config {
  asr_model: string;
  asr_language: string;
  asr_device: string;
  whisper_path: string;
  whisper_gpu_path: string;
  models_dir: string;
  cleanup_mode: string;
  cleanup_device: string;
  cleanup_model: string;
  ollama_url: string;
  translate_to_english: boolean;
  auto_paste: boolean;
  max_recording_secs: number;
}

interface SetupStatus {
  has_whisper_cpu: boolean;
  has_whisper_gpu: boolean;
  has_model: boolean;
  model_name: string;
  needs_setup: boolean;
}

// --- Setup screen elements ---
const setupScreen = document.getElementById("setup-screen")!;
const setupBtn = document.getElementById("setup-btn")!;
const setupProgress = document.getElementById("setup-progress")!;
const setupStatusText = document.getElementById("setup-status-text")!;
const progressBarFill = document.getElementById("progress-bar-fill")!;
const progressDetail = document.getElementById("progress-detail")!;
const setupError = document.getElementById("setup-error")!;
const cudaCheck = document.getElementById("cuda-check") as HTMLInputElement;

// --- Main app elements ---
const appDiv = document.getElementById("app")!;
const statusIndicator = document.getElementById("status-indicator")!;
const statusText = document.getElementById("status-text")!;
const transcriptList = document.getElementById("transcript-list")!;
const asrModelSelect = document.getElementById("asr-model-select") as HTMLSelectElement;
const asrDeviceSelect = document.getElementById("asr-device-select") as HTMLSelectElement;
const cleanupModeSelect = document.getElementById("cleanup-mode-select") as HTMLSelectElement;
const cleanupModelSelect = document.getElementById("cleanup-model-select") as HTMLSelectElement;
const saveBtn = document.getElementById("save-btn")!;
const saveStatus = document.getElementById("save-status")!;

const MAX_HISTORY = 5;

interface TranscriptEntry {
  text: string;
  timing: string;
}

const history: TranscriptEntry[] = [];

let currentConfig: Config;

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(0) + " KB";
  if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(1) + " MB";
  return (bytes / (1024 * 1024 * 1024)).toFixed(2) + " GB";
}

// ========================
// Setup flow
// ========================

async function checkAndShowSetup() {
  const status = await invoke<SetupStatus>("check_setup");

  if (status.needs_setup) {
    setupScreen.style.display = "flex";
    appDiv.style.display = "none";

    setupBtn.addEventListener("click", () => startSetup());

    // Listen for progress events
    await listen<{ step: string; downloaded: number; total: number }>("setup-progress", (event) => {
      const { downloaded, total } = event.payload;
      if (total > 0) {
        const pct = Math.round((downloaded / total) * 100);
        progressBarFill.style.width = pct + "%";
        progressDetail.textContent = `${formatBytes(downloaded)} / ${formatBytes(total)} (${pct}%)`;
      }
    });

    await listen<string>("setup-status", (event) => {
      setupStatusText.textContent = event.payload;
    });
  } else {
    setupScreen.style.display = "none";
    appDiv.style.display = "flex";
    await initApp();
  }
}

async function startSetup() {
  setupBtn.style.display = "none";
  setupProgress.style.display = "block";
  setupError.style.display = "none";

  const includeCuda = cudaCheck.checked;

  try {
    await invoke("run_setup", { includeCuda });
    // Setup done — switch to main app
    setupScreen.style.display = "none";
    appDiv.style.display = "flex";
    await initApp();
  } catch (e) {
    setupError.textContent = `Setup failed: ${e}`;
    setupError.style.display = "block";
    setupBtn.style.display = "block";
    setupBtn.textContent = "Retry";
  }
}

// ========================
// Main app
// ========================

function renderHistory() {
  transcriptList.innerHTML = "";
  if (history.length === 0) {
    transcriptList.innerHTML = '<div class="transcript-entry transcript-empty">\u2014</div>';
    return;
  }
  for (let i = 0; i < history.length; i++) {
    const entry = history[i];
    const div = document.createElement("div");
    div.className = "transcript-entry" + (i === 0 ? " latest" : " older");

    const timingDiv = document.createElement("div");
    timingDiv.className = "timing";
    timingDiv.textContent = entry.timing;

    const textSpan = document.createElement("span");
    textSpan.className = "transcript-text";
    textSpan.textContent = entry.text;

    const copyBtn = document.createElement("button");
    copyBtn.className = "copy-entry-btn";
    copyBtn.textContent = "Copy";
    copyBtn.title = "Copy transcript";
    copyBtn.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(entry.text);
        copyBtn.textContent = "Copied!";
        setTimeout(() => { copyBtn.textContent = "Copy"; }, 1500);
      } catch {
        copyBtn.textContent = "Failed";
        setTimeout(() => { copyBtn.textContent = "Copy"; }, 1500);
      }
    });

    const header = document.createElement("div");
    header.className = "transcript-header";
    header.appendChild(timingDiv);
    header.appendChild(copyBtn);

    div.appendChild(header);
    div.appendChild(textSpan);
    transcriptList.appendChild(div);
  }
}

function formatModelLabel(model: string): string {
  if (model.endsWith(".en")) {
    return `${model} — English only`;
  }
  return `${model} — EN/CN multilingual`;
}

async function refreshOllamaModels() {
  const selected = cleanupModelSelect.value || currentConfig?.cleanup_model || "";
  cleanupModelSelect.innerHTML = "";
  try {
    const ollamaModels = await invoke<string[]>("list_ollama_models");
    for (const model of ollamaModels) {
      const opt = document.createElement("option");
      opt.value = model;
      opt.textContent = model;
      cleanupModelSelect.appendChild(opt);
    }
  } catch {
    // Ollama not running
  }
  if (selected && !Array.from(cleanupModelSelect.options).some(o => o.value === selected)) {
    const opt = document.createElement("option");
    opt.value = selected;
    opt.textContent = selected;
    cleanupModelSelect.appendChild(opt);
  }
  if (selected) cleanupModelSelect.value = selected;
}

function setStatus(status: string) {
  statusIndicator.className = status;
  statusText.textContent = status.charAt(0).toUpperCase() + status.slice(1);
}

async function loadConfig() {
  currentConfig = await invoke<Config>("get_config");

  const models = await invoke<string[]>("list_models");
  asrModelSelect.innerHTML = "";
  for (const model of models) {
    const opt = document.createElement("option");
    opt.value = model;
    opt.textContent = formatModelLabel(model);
    asrModelSelect.appendChild(opt);
  }
  if (!models.includes(currentConfig.asr_model)) {
    const opt = document.createElement("option");
    opt.value = currentConfig.asr_model;
    opt.textContent = `${currentConfig.asr_model} (not downloaded)`;
    asrModelSelect.appendChild(opt);
  }
  asrModelSelect.value = currentConfig.asr_model;

  asrDeviceSelect.value = currentConfig.asr_device;
  cleanupModeSelect.value = currentConfig.cleanup_mode;

  await refreshOllamaModels();
}

async function saveConfig() {
  const newConfig: Config = {
    ...currentConfig,
    asr_model: asrModelSelect.value,
    asr_device: asrDeviceSelect.value,
    asr_language: asrModelSelect.value.endsWith(".en") ? "en" : "auto",
    translate_to_english: cleanupModeSelect.value !== "off",
    cleanup_mode: cleanupModeSelect.value,
    cleanup_model: cleanupModelSelect.value,
  };

  try {
    await invoke("update_config", { newConfig });
    currentConfig = newConfig;
    saveStatus.textContent = "Saved!";
    saveStatus.style.color = "#4ade80";
    setTimeout(() => { saveStatus.textContent = ""; }, 2000);
  } catch (e) {
    saveStatus.textContent = `Error: ${e}`;
    saveStatus.style.color = "#ef4444";
  }
}

async function initApp() {
  await loadConfig();

  renderHistory();

  saveBtn.addEventListener("click", saveConfig);
  cleanupModelSelect.addEventListener("focus", refreshOllamaModels);

  await listen<string>("recording-status", (event) => {
    setStatus(event.payload);
  });

  await listen<{ text: string; asr_device: string; asr_ms: number; cleanup_ms: number | null }>("transcription-result", (event) => {
    const { text, asr_device, asr_ms, cleanup_ms } = event.payload;
    const asrStr = (asr_ms / 1000).toFixed(1);
    const llmStr = cleanup_ms != null ? (cleanup_ms / 1000).toFixed(1) + "s" : "\u2014";
    const timing = `${asr_device} ASR ${asrStr}s | LLM ${llmStr}`;
    history.unshift({ text, timing });
    if (history.length > MAX_HISTORY) history.pop();
    renderHistory();
  });

  await listen<string>("error", (event) => {
    console.error("Unmute error:", event.payload);
  });
}

// Entry point: check setup first, then show app or setup screen
checkAndShowSetup();
