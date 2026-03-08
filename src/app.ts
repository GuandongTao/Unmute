import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Config {
  asr_model: string;
  asr_language: string;
  whisper_path: string;
  models_dir: string;
  cleanup_mode: string;
  cleanup_device: string;
  cleanup_model: string;
  ollama_url: string;
  auto_paste: boolean;
  max_recording_secs: number;
}

const statusIndicator = document.getElementById("status-indicator")!;
const statusText = document.getElementById("status-text")!;
const rawTranscript = document.getElementById("raw-transcript")!;
const asrModelSelect = document.getElementById("asr-model-select") as HTMLSelectElement;
const cleanupModeSelect = document.getElementById("cleanup-mode-select") as HTMLSelectElement;
const cleanupModelSelect = document.getElementById("cleanup-model-select") as HTMLSelectElement;
const saveBtn = document.getElementById("save-btn")!;
const saveStatus = document.getElementById("save-status")!;

let currentConfig: Config;

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
  // Ensure current/selected model is in the list
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

  // Populate model dropdown
  const models = await invoke<string[]>("list_models");
  asrModelSelect.innerHTML = "";
  for (const model of models) {
    const opt = document.createElement("option");
    opt.value = model;
    opt.textContent = model;
    asrModelSelect.appendChild(opt);
  }
  // If current model not in list, add it
  if (!models.includes(currentConfig.asr_model)) {
    const opt = document.createElement("option");
    opt.value = currentConfig.asr_model;
    opt.textContent = `${currentConfig.asr_model} (not downloaded)`;
    asrModelSelect.appendChild(opt);
  }
  asrModelSelect.value = currentConfig.asr_model;

  cleanupModeSelect.value = currentConfig.cleanup_mode;

  await refreshOllamaModels();
}

async function saveConfig() {
  const newConfig: Config = {
    ...currentConfig,
    asr_model: asrModelSelect.value,
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

async function init() {
  await loadConfig();

  saveBtn.addEventListener("click", saveConfig);
  cleanupModelSelect.addEventListener("focus", refreshOllamaModels);

  await listen<string>("recording-status", (event) => {
    setStatus(event.payload);
  });

  await listen<{ text: string; asr_ms: number; cleanup_ms: number | null }>("transcription-result", (event) => {
    const { text, asr_ms, cleanup_ms } = event.payload;
    const asrStr = (asr_ms / 1000).toFixed(1);
    const llmStr = cleanup_ms != null ? (cleanup_ms / 1000).toFixed(1) + "s" : "\u2014";
    rawTranscript.innerHTML = `<div class="timing">ASR ${asrStr}s | LLM ${llmStr}</div>${text}`;
  });

  await listen<string>("error", (event) => {
    rawTranscript.textContent = `Error: ${event.payload}`;
    console.error("Unmute error:", event.payload);
  });
}

init();
