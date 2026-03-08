#!/bin/bash
# Unmute Setup - Bash wrapper for Windows (Git Bash / MSYS2)
# For full options, use: powershell -ExecutionPolicy Bypass -File scripts/setup.ps1

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
powershell.exe -ExecutionPolicy Bypass -File "$SCRIPT_DIR/setup.ps1" "$@"
