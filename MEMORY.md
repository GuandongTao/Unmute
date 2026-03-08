# Unmute Development Memory

## Project Overview
- Tauri (Rust + WebView) local voice typing app for Windows
- Push-to-talk dictation with whisper.cpp ASR + optional Ollama LLM cleanup
- Hardware: i7-13700K, 32GB RAM, RTX 3080 12GB

## Current State
- All modules implemented and working: main.rs, audio.rs, asr.rs, cleanup.rs, paste.rs, config.rs, logger.rs, hotkey.rs
- Frontend: plain HTML/TS, minimal debug UI + floating overlay pill
- Project builds and runs successfully
- No git commits yet

## Completed: Low-Level Keyboard Hook (replaced tauri-plugin-global-shortcut)
- `tauri-plugin-global-shortcut` couldn't do modifier-only combos or left/right distinction
- Implemented WH_KEYBOARD_LL hook in `hotkey.rs` using `windows` crate
- Hold-to-talk: Left Alt + Right Ctrl — TESTED WORKING
- Toggle-to-talk: Left Alt + Right Ctrl + Right Shift — working, debounced (500ms)
- Key files: hotkey.rs, main.rs, config.rs (removed hotkey string fields), capabilities/default.json

## Completed: Floating Overlay Pill
- 80x28px transparent always-on-top click-through window, center-bottom above taskbar
- States: idle (dim green), recording (bright green), processing (yellow), error (red)
- Color updates via `eval()` on overlay webview (Tauri event listeners didn't work)
- Files: src/overlay.html, main.rs (set_overlay_status helper)

## Completed: Bug Fixes from First Test
- Toggle rapid-fire: fixed with `!prev_rshift` check + 500ms debounce
- `<|endoftext|>` in output: added strip_special_tokens() in asr.rs
- Whisper flags: `--no-prints` instead of `--print-special false`

## Completed: Pill Timing Display
- After processing, pill expands to show `ASR 3.7s | LLM —` for ~5s, then shrinks back
- Uses set_overlay_done() with eval() — passes timing from ProcessResult struct
- Overlay window sized 200x28 to accommodate expanded pill text

## Completed: Settings GUI
- Main window has settings form: ASR model dropdown, cleanup mode/model/device, auto-paste
- `list_models` Tauri command scans models_dir for ggml-*.bin files
- Save button calls `update_config` to persist to config.json
- Both `base.en` (148MB, faster) and `small.en` (488MB, more accurate) downloaded

## Completed: Ollama Setup for LLM Cleanup
- Ollama installed via silent installer to `%LOCALAPPDATA%/Programs/Ollama/`
- Auto-starts as background service after install
- Pulled `qwen2.5:3b` (Q4_K_M, ~1.9GB) — good balance of speed and quality for cleanup
- Config updated: cleanup_model changed from `qwen3.5:0.8b` to `qwen2.5:3b`
- Tested: Ollama API responds on localhost:11434
- LLM cleanup should now work end-to-end when cleanup_mode is "light" or "rewrite"

## Future / Planned Features
- Custom hotkey recording: click button to enable recording mode, capture keys on release, save as new hotkey config
- Background music transcribed as [MUSIC]/[BLANK_AUDIO] — whisper picks up non-speech audio

## Known Issues
- Whisper sometimes misses punctuation on short clips — Light cleanup mode fixes this
- Toggle mode needs more testing
- Model benchmarking: small.en (~3.5s for short clips) vs base.en (expected ~1.5-2s)

## Technical Notes
- windows crate 0.61: HHOOK uses `*mut c_void`, CallNextHookEx takes `Option<HHOOK>`
- Hook callback must not block >200ms or Windows silently removes it
- Overlay color: Tauri event listeners didn't reach overlay window; using `WebviewWindow::eval()` instead
- Config file: `%APPDATA%/unmute/config.json`
- vite.config.ts: multi-page build with rollupOptions for overlay.html
