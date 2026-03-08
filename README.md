# Unmute

Local voice typing for Windows. Push-to-talk dictation that runs entirely on your machine — no cloud, no subscriptions.

## What it does

1. Press a hotkey to start recording from your microphone
2. Release to transcribe speech to text using [whisper.cpp](https://github.com/ggml-org/whisper.cpp)
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
- **ASR CPU/GPU toggle** — run whisper.cpp on CPU or GPU (NVIDIA CUDA); switchable from the GUI
- **Automatic GPU-to-CPU fallback** — if the CUDA build fails, ASR retries on CPU automatically
- **LLM cleanup** — optional transcript cleanup via Ollama (light or rewrite modes)
- **Clipboard-safe** — saves and restores your clipboard content (including images) after pasting
- **Auto-starts Ollama** — no need to manually manage the Ollama service
- **Settings GUI** — change ASR model/device, cleanup mode, and cleanup model from the app

## Resource usage

| Component | Runs on | Impact | When |
|-----------|---------|--------|------|
| **ASR (whisper.cpp)** | CPU or GPU (configurable) | CPU: ~4-6 cores for 3-4s; GPU: ~1s with CUDA | Only during transcription |
| **LLM cleanup (Ollama)** | GPU | ~2GB VRAM, sub-second inference | Only during cleanup |
| **Idle** | — | Near-zero CPU/RAM | Always |

**Designed to coexist with GPU workloads.** When ASR is set to CPU mode, it won't touch your GPU at all — safe for training, gaming, or rendering. GPU ASR uses CUDA briefly (~1s) per transcription. LLM cleanup uses the GPU briefly but is optional — disable it (`cleanup_mode: off`) if your GPU is fully occupied.

## Prerequisites

- **Windows 10/11** (x64)
- **Microphone** — any USB or built-in mic

### For ASR (required)
- [whisper.cpp](https://github.com/ggml-org/whisper.cpp/releases) CPU binary (downloaded automatically by setup script)
- A GGML whisper model (downloaded automatically — default: `small.en`, ~466MB)

### For GPU-accelerated ASR (optional)
- **NVIDIA GPU** with CUDA support (GTX 1060+, RTX series, etc.)
- CUDA toolkit is **not** required — the whisper.cpp CUDA build bundles its own runtime DLLs
- **AMD GPUs are not supported** for ASR acceleration — use CPU mode instead

### For LLM transcript cleanup (optional)
- [Ollama](https://ollama.com/download) — the app auto-starts it if installed
- A small LLM model (default: `qwen2.5:3b`, ~1.9GB download)
- Requires a GPU with ~2GB free VRAM for fast inference

## Quick setup

```powershell
# Run the setup script — downloads whisper.cpp (CPU + CUDA builds) and ASR model
powershell -ExecutionPolicy Bypass -File scripts\setup.ps1

# Optional flags:
#   -SkipCuda       Skip CUDA build download (AMD GPU or no NVIDIA GPU)
#   -SkipOllama     Skip Ollama model pull
#   -ModelSize      Change ASR model (default: small.en)

# Optional: install Ollama for LLM cleanup
# Download from: https://ollama.com/download
# Then: ollama pull qwen2.5:3b
```

### GPU compatibility

| GPU | ASR (whisper.cpp) | LLM cleanup (Ollama) |
|-----|-------------------|---------------------|
| **NVIDIA (CUDA)** | GPU or CPU (your choice) | GPU |
| **AMD / Intel** | CPU only | GPU (via Ollama's ROCm/Vulkan support) |
| **No GPU** | CPU only | CPU (slow) or disable cleanup |

If ASR is set to GPU mode but the CUDA build fails (wrong GPU, missing driver, etc.), it automatically falls back to CPU. The GUI will show "CPU*" to indicate fallback occurred.

## Development

```bash
npm install
npx tauri dev
```

## Build

```bash
npx tauri build
# Output: src-tauri/target/release/unmute.exe (~14MB standalone)
# Installer: src-tauri/target/release/bundle/nsis/unmute_*_x64-setup.exe (~3MB)
```

## Architecture

Built with [Tauri v2](https://tauri.app) (Rust backend + WebView frontend).

| Module | Purpose |
|--------|---------|
| `hotkey.rs` | Low-level keyboard hook (`WH_KEYBOARD_LL`) for modifier-only hotkeys with left/right key distinction |
| `audio.rs` | Microphone capture via cpal, resamples to 16kHz mono WAV |
| `asr.rs` | whisper.cpp subprocess wrapper with GPU-to-CPU fallback |
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
  "asr_device": "cpu",
  "whisper_path": "C:\\Users\\...\\unmute\\bin\\whisper-cli.exe",
  "whisper_gpu_path": "C:\\Users\\...\\unmute\\bin-gpu\\whisper-cli.exe",
  "models_dir": "C:\\Users\\...\\unmute\\models",
  "cleanup_mode": "off",
  "cleanup_model": "qwen2.5:3b",
  "ollama_url": "http://localhost:11434",
  "auto_paste": true,
  "max_recording_secs": 120
}
```

| Key | Values | Description |
|-----|--------|-------------|
| `asr_device` | `cpu`, `gpu` | Which device to run whisper.cpp on |
| `cleanup_mode` | `off`, `light`, `rewrite` | `light` = fix punctuation/fillers, `rewrite` = restructure for clarity |
| `whisper_path` | path | CPU whisper-cli.exe location |
| `whisper_gpu_path` | path | CUDA whisper-cli.exe location (leave empty if no NVIDIA GPU) |

## License

MIT
