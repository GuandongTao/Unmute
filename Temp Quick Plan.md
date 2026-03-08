Project: Unmute — Local Voice Typing with ASR + Optional LLM Cleanup
Goal: Build a minimal, lightweight desktop app for push-to-talk dictation that runs fully locally on Windows. Designed to coexist with GPU-heavy workloads (gaming, training).

Hardware context
- CPU: Intel 13700K (8P + 8E cores)
- RAM: 32GB
- GPU: RTX 3080 12GB — shared with gaming/training
- ASR runs on CPU (whisper.cpp, model kept resident in RAM)
- LLM cleanup runs on GPU via Ollama (~2GB VRAM), with CPU-only fallback option
- App must have minimal footprint as a background process

Usage context
- Dictation clips are short — never over 2 minutes
- Toggle mode auto-stops recording at 2-minute mark
- Typical usage: 5-30 second clips

High-level product behavior
1. User presses and holds a global hotkey (default: Left Alt + Right Ctrl).
2. App records microphone audio locally.
3. On hotkey release, app transcribes audio with a local ASR model (CPU).
4. App pastes raw transcript into the active app immediately.
5. If cleanup is enabled (off by default), app sends transcript to a local LLM:
   - removes filler words
   - fixes grammar
   - improves sentence structure
   - preserves English/Chinese code-switching
   - does not add new information
   - replaces pasted text with cleaned version when done (pipeline approach)
6. App stores raw + cleaned transcripts in log files for debugging.

Hotkey configuration
- Hold-to-record (default): Left Alt + Right Ctrl
- Toggle mode: Left Alt + Right Ctrl + Right Shift
  - auto-stops at 2-minute continuous recording mark
- Hotkeys are user-configurable in settings

Core product constraints
- 100% local, no cloud dependencies
- ASR on CPU, cleanup LLM on GPU (~2GB VRAM) with CPU fallback
- English-only models for ASR; bilingual EN/ZH is nice-to-have, not critical
- low-latency enough for daily use
- modular: ASR and cleanup engines swappable via adapter interfaces
- minimal system resource usage — must run comfortably alongside gaming

Important design decision
Do NOT fork the entire OpenWhispr codebase as the product foundation.
Instead:
- use OpenWhispr as a reference implementation
- selectively extract or reimplement only the needed modules
- keep the new app thin and purpose-built

Reasoning
OpenWhispr is useful because it already solved:
- global hotkeys
- audio recording flow
- temp-file based transcription pipeline
- cross-platform clipboard/paste behavior
- local whisper/parakeet/llama.cpp integration

But it also contains much broader product scope:
- notes, agents, calendar, cloud integrations, settings complexity, multi-window app surface

We want a smaller codebase with cleaner ownership.

---

Tech stack decisions

Framework: Tauri (Rust + WebView)
- ~10MB install vs Electron's ~150MB+
- ~30MB RAM vs Electron's ~200MB+
- native system tray, global hotkeys, clipboard APIs built in
- Windows-first, Mac expansion later (Tauri supports both)
- UI: plain HTML/CSS/TS — no React, no bundler complexity
- Settings: single JSON config file

ASR: whisper.cpp (CPU)
- most stable and proven local whisper runtime on Windows
- default model: `small.en` (~460MB, English-only, faster + more accurate than multilingual)
- fallback model: `base.en` (~140MB, even faster, slightly less accurate)
- runs as subprocess — spawn whisper-cli, parse stdout
- expected latency: ~2-5s for 10s audio on modern CPU

Cleanup LLM: Ollama (GPU default, CPU fallback, optional)
- easiest local LLM setup — single installer, HTTP API, model management
- default model: Qwen3.5 0.8B Q4 (~1.5GB VRAM on GPU, <1s for short transcripts)
- step-up model: Qwen3 1.7B Q4 (~2GB VRAM, if 0.8B quality disappoints)
- fallback: CPU-only mode (~2GB RAM, ~5s latency — for when GPU is fully occupied)
- OFF by default — user opts in via settings
- pipeline UX: paste raw first, replace with cleaned text when ready
- setting to choose: gpu / cpu / off

Audio: Web Audio API via WebView
- captures from default system mic
- option to select specific mic in settings (future)
- saves to temp WAV file, 16kHz mono (whisper requirement)

---

Phase 0 — OpenWhispr reconnaissance (reference only, do as needed)
1. Clone OpenWhispr into `reference/openwhispr` (gitignored).
2. Consult these files when implementing equivalent features:
   - main.js, preload.js
   - src/helpers/hotkeyManager.js
   - src/helpers/clipboard.js
   - src/helpers/audioManager.js
   - src/helpers/whisper.js
   - src/helpers/modelManagerBridge.js
3. Do NOT do a full audit upfront. Reference as needed during implementation.

Expected reuse:
- hotkeyManager patterns → adapt for Tauri global shortcut plugin
- clipboard/paste logic → adapt for Tauri clipboard API
- whisper invocation logic → adapt subprocess spawning
- model download helper ideas → simplified downloader

Do NOT reuse:
- broad renderer UI
- database-heavy product features
- cloud AI/provider abstraction
- notes/agent/calendar systems

---

Phase 1 — Project scaffold

unmute/
  src-tauri/           # Rust backend
    src/
      main.rs          # app entry, tray, global shortcuts
      audio.rs         # mic capture coordination
      asr.rs           # whisper.cpp subprocess wrapper
      cleanup.rs       # ollama HTTP client
      paste.rs         # clipboard write + paste simulation
      config.rs        # JSON config read/write
      logger.rs        # structured logging
    tauri.conf.json
    Cargo.toml
  src/                 # Frontend (plain HTML/TS)
    index.html
    app.ts             # minimal UI logic
    style.css
  scripts/             # Model download helpers
  reference/           # OpenWhispr reference (gitignored)
  package.json
  .gitignore

---

Phase 2 — Minimal vertical slice (MVP)

A. Global hotkey
- register Left Alt + Right Ctrl as hold-to-record
- key down → start recording
- key up → stop recording, trigger ASR
- state machine: idle → recording → processing → idle

B. Audio capture
- capture from default mic via Tauri backend (cpal crate or similar)
- save to temp WAV file (16kHz, mono, PCM)
- delete temp file after processing

C. ASR adapter
interface (Rust trait):
  fn transcribe(audio_path: &str, language: &str) -> Result<Transcript>

Transcript {
  text: String,
  segments: Option<Vec<Segment>>,  // for future use
}

First implementation:
- spawn whisper.cpp CLI process
- pass temp WAV file path
- parse stdout for transcript text
- default model: small.en
- model path configurable in settings

D. Paste
- write transcript to clipboard
- simulate Ctrl+V to paste into active app
- restore previous clipboard content after short delay
- configurable: auto-paste on/off

E. Cleanup adapter (optional, off by default)
interface (Rust trait):
  fn cleanup(text: &str, mode: CleanupMode) -> Result<String>

CleanupMode: Off | Light | Rewrite

First implementation:
- HTTP POST to Ollama API (localhost:11434)
- model: qwen2.5:3b or phi3.5
- pipeline: paste raw first, then replace with cleaned text

---

Phase 3 — Cleanup prompts & bilingual handling (merged)

Mode: light
System prompt:
  You are a transcript cleanup assistant.
  Clean the text lightly.
  Remove filler words and false starts.
  Fix punctuation and grammar.
  Preserve wording and meaning.
  Keep English and Chinese exactly as used.
  Do not translate.
  Do not add information.
  Output only the cleaned text.

Mode: rewrite
System prompt:
  You are a transcript-to-prose assistant.
  Rewrite the transcript into clear written sentences.
  Remove filler words, repetition, and broken fragments.
  Improve sentence structure and readability.
  Preserve meaning exactly.
  Keep English and Chinese mixed usage if present.
  Do not translate unless the user explicitly asks.
  Do not add new information.
  Output only the rewritten text.

Safety rule:
- if transcript is < 5 words or confidence is low, return near-literal output

Language settings:
- ASR language: default `en`, option for `auto` or `zh` in settings
- cleanup prompts preserve mixed-language text
- raw ASR output saved in logs for debugging

---

Phase 4 — Debugging and observability

Structured log file (JSON lines):
- timestamp
- recording duration
- audio file path
- ASR model used
- ASR latency (ms)
- raw transcript
- cleanup mode + model (if used)
- cleanup latency (ms)
- final pasted text
- errors/fallbacks

Minimal debug panel in UI:
- latest raw transcript
- latest cleaned transcript (if cleanup on)
- latency numbers
- current settings (hotkey, model, cleanup mode)
- status indicator (idle / recording / processing)

---

Phase 5 — Testing matrix

Manual test scenarios:
1. Clean English dictation
2. Clean Mandarin dictation
3. Mixed Chinese + English product terms
4. Fast speech
5. Long rambling sentence with fillers
6. Background noise (fan, gaming audio)
7. Paste targets: Notepad, browser textarea, Slack/Discord, code editor
8. Cleanup off vs light vs rewrite
9. ASR base.en vs small.en
10. Running alongside a game (resource impact)

Measure:
- ASR latency
- cleanup latency
- total time: key release → text pasted
- accuracy (English focus)
- system resource impact while gaming

---

Phase 6 — Post-MVP optimization

1. Benchmark ASR models on CPU:
   - base.en (fastest, less accurate)
   - small.en (balanced)
   - medium.en (if CPU can handle it)
2. Benchmark cleanup models on CPU:
   - Qwen2.5 3B instruct 4-bit
   - Phi-3.5 mini 4-bit
   - Consider skipping cleanup if latency is too high on CPU
3. Explore streaming transcription (whisper.cpp --stream) for real-time preview
4. Consider "raw transcript preview before paste" mode
5. Mac support via Tauri cross-platform

---

Definition of done for MVP
- hold-to-record hotkey works globally
- records from default mic
- local ASR via whisper.cpp on CPU
- pastes transcript into active app
- optional LLM cleanup (off by default)
- works offline, no cloud
- good enough for daily English dictation
- runs comfortably alongside games
- codebase is small and understandable
- < 50MB installed size (excluding models)

---

Resource budget targets
- Idle: < 20MB RAM + ~500MB (whisper model resident), ~0% CPU
- Recording: same + minimal CPU for audio capture
- ASR processing: ~500MB RAM, 4-6 P-cores for ~2-5s (short clips typically ~2s)
- Cleanup (GPU mode): ~1.5GB VRAM (Qwen3.5 0.8B), <1s
- Cleanup (CPU fallback): ~2GB RAM, 2-4 cores, ~5s
- Cleanup (off): no additional cost

---

Resolved decisions
1. Toggle hotkey: Left Alt + Right Ctrl + Right Shift (with 2-min auto-stop)
2. Whisper model: keep resident in memory (~500MB) — 32GB RAM makes this trivial
3. Ollama model caching: use default behavior (keeps model loaded ~5min after last request). Acceptable given GPU VRAM headroom.

---

Deliverables
1. Tauri project scaffold
2. Working vertical slice (hotkey → record → ASR → paste)
3. Optional cleanup pipeline
4. Adapted logic from OpenWhispr where useful (with attribution)
5. Benchmark report for model choices on CPU
6. Next-step recommendations after MVP
