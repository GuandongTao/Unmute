# Unmute

Local voice typing for Windows. Push-to-talk dictation that runs entirely on your machine — no cloud, no subscriptions.

## What it does

1. Press a hotkey to start recording from your microphone
2. Release to transcribe speech to text using [whisper.cpp](https://github.com/ggml-org/whisper.cpp) (runs on CPU)
3. Text is automatically pasted into the active application
4. Optionally clean up the transcript with a local LLM via [Ollama](https://ollama.com) (removes filler words, fixes grammar)

## Hotkeys

| Action | Keys |
|--------|------|
| Hold-to-talk | `Left Alt` + `Right Ctrl` (hold while speaking, release to transcribe) |
| Toggle-to-talk | `Left Alt` + `Right Ctrl` + `Right Shift` (press to start, press again to stop) |

Toggle mode auto-stops after 2 minutes.

## Features

- **Fully local** — no internet required, no data leaves your machine
- **Lightweight** — Tauri app (~10MB), minimal resource usage when idle
- **Floating overlay** — small pill indicator shows recording/processing/idle state with timing
- **LLM cleanup** — optional transcript cleanup via Ollama (light or rewrite modes)
- **Clipboard-safe** — saves and restores your clipboard content (including images) after pasting
- **Auto-starts Ollama** — no need to manually manage the Ollama service
- **Settings GUI** — change ASR model, cleanup mode, and cleanup model from the app

## Resource usage

| Component | Runs on | Impact | When |
|-----------|---------|--------|------|
| **ASR (whisper.cpp)** | CPU only | ~4-6 cores for 3-4s per transcription | Only during transcription |
| **LLM cleanup (Ollama)** | GPU (default) | ~2GB VRAM, sub-second inference | Only during cleanup |
| **Idle** | — | Near-zero CPU/RAM | Always |

**Designed to coexist with GPU workloads.** ASR runs entirely on CPU, so it won't interfere with GPU training, gaming, or rendering. LLM cleanup uses the GPU briefly but is optional — disable it (`cleanup_mode: off`) if your GPU is fully occupied.

## Requirements

- Windows 10/11
- [whisper.cpp](https://github.com/ggml-org/whisper.cpp/releases) CLI binary + a GGML model
- [Ollama](https://ollama.com/download) (optional, for LLM cleanup)

## Quick setup

```powershell
# Run the setup script to download whisper.cpp and a model
powershell -ExecutionPolicy Bypass -File scripts\setup.ps1

# Optional: install Ollama and pull a cleanup model
# https://ollama.com/download
ollama pull qwen2.5:3b
```

## Development

```bash
npm install
npx tauri dev
```

## Build

```bash
npx tauri build
```

## Architecture

Built with [Tauri v2](https://tauri.app) (Rust backend + WebView frontend).

| Module | Purpose |
|--------|---------|
| `hotkey.rs` | Low-level keyboard hook (`WH_KEYBOARD_LL`) for modifier-only hotkeys with left/right key distinction |
| `audio.rs` | Microphone capture via cpal, resamples to 16kHz mono WAV |
| `asr.rs` | whisper.cpp subprocess wrapper, strips special tokens |
| `cleanup.rs` | Ollama HTTP client for transcript cleanup (light/rewrite modes) |
| `paste.rs` | Clipboard save/restore + simulated Ctrl+V paste |
| `config.rs` | JSON config at `%APPDATA%/unmute/config.json` |
| `logger.rs` | Structured JSON logs at `%LOCALAPPDATA%/unmute/logs/` |

## Config

Config lives at `%APPDATA%/unmute/config.json`:

```json
{
  "asr_model": "small.en",
  "asr_language": "en",
  "cleanup_mode": "off",
  "cleanup_model": "qwen2.5:3b",
  "auto_paste": true,
  "max_recording_secs": 120
}
```

Cleanup modes: `off`, `light` (fix punctuation/fillers), `rewrite` (restructure for clarity).

## License

MIT
