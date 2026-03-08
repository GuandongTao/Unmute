import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
const statusIndicator = document.getElementById("status-indicator");
const statusText = document.getElementById("status-text");
const rawTranscript = document.getElementById("raw-transcript");
const holdHotkey = document.getElementById("hold-hotkey");
const toggleHotkey = document.getElementById("toggle-hotkey");
const asrModel = document.getElementById("asr-model");
const cleanupMode = document.getElementById("cleanup-mode");
const asrLatency = document.getElementById("asr-latency");
function setStatus(status) {
    statusIndicator.className = status;
    statusText.textContent = status.charAt(0).toUpperCase() + status.slice(1);
}
async function init() {
    // Load config
    const config = await invoke("get_config");
    holdHotkey.textContent = config.hotkey_hold;
    toggleHotkey.textContent = config.hotkey_toggle;
    asrModel.textContent = config.asr_model;
    cleanupMode.textContent = config.cleanup_mode;
    // Listen for events from backend
    await listen("recording-status", (event) => {
        setStatus(event.payload);
    });
    await listen("transcription-result", (event) => {
        rawTranscript.textContent = event.payload;
    });
    await listen("error", (event) => {
        rawTranscript.textContent = `Error: ${event.payload}`;
        console.error("Unmute error:", event.payload);
    });
}
init();
