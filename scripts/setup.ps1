# Unmute Setup Script
# Downloads whisper.cpp, ASR model, and optionally sets up Ollama
# Run: powershell -ExecutionPolicy Bypass -File scripts\setup.ps1

param(
    [switch]$SkipWhisper,
    [switch]$SkipModel,
    [switch]$SkipOllama,
    [switch]$CudaBuild,
    [string]$ModelSize = "small.en"
)

$ErrorActionPreference = "Stop"

$AppDir = Join-Path $env:LOCALAPPDATA "unmute"
$BinDir = Join-Path $AppDir "bin"
$ModelsDir = Join-Path $AppDir "models"
$WhisperVersion = "v1.8.3"
$WhisperRepo = "ggml-org/whisper.cpp"
$HfRepo = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main"

Write-Host ""
Write-Host "=== Unmute Setup ===" -ForegroundColor Cyan
Write-Host "App directory: $AppDir"
Write-Host ""

# Create directories
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
New-Item -ItemType Directory -Force -Path $ModelsDir | Out-Null

# --- Step 1: Download whisper.cpp ---
if (-not $SkipWhisper) {
    $whisperExe = Join-Path $BinDir "whisper-cli.exe"

    if (Test-Path $whisperExe) {
        Write-Host "[whisper.cpp] Already installed at $whisperExe" -ForegroundColor Green
    } else {
        if ($CudaBuild) {
            $zipName = "whisper-cublas-12.4.0-bin-x64.zip"
        } else {
            $zipName = "whisper-bin-x64.zip"
        }

        $downloadUrl = "https://github.com/$WhisperRepo/releases/download/$WhisperVersion/$zipName"
        $zipPath = Join-Path $env:TEMP "whisper-cpp.zip"

        Write-Host "[whisper.cpp] Downloading $zipName..." -ForegroundColor Yellow
        Write-Host "  URL: $downloadUrl"

        try {
            Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath -UseBasicParsing
        } catch {
            Write-Host "[whisper.cpp] Download failed. Trying without version..." -ForegroundColor Red
            Write-Host "  Please download manually from: https://github.com/$WhisperRepo/releases" -ForegroundColor Red
            Write-Host "  Extract whisper-cli.exe to: $BinDir" -ForegroundColor Red
            $SkipWhisper = $true
        }

        if (-not $SkipWhisper) {
            Write-Host "[whisper.cpp] Extracting..." -ForegroundColor Yellow
            $extractDir = Join-Path $env:TEMP "whisper-extract"
            if (Test-Path $extractDir) { Remove-Item -Recurse -Force $extractDir }
            Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

            # Find and copy the main executable
            $exeFiles = Get-ChildItem -Path $extractDir -Recurse -Filter "whisper-cli.exe"
            if ($exeFiles.Count -eq 0) {
                # Older versions may name it differently
                $exeFiles = Get-ChildItem -Path $extractDir -Recurse -Filter "main.exe"
            }

            if ($exeFiles.Count -gt 0) {
                Copy-Item $exeFiles[0].FullName -Destination $whisperExe -Force
                # Also copy any DLLs (needed for CUDA builds)
                $dlls = Get-ChildItem -Path $exeFiles[0].DirectoryName -Filter "*.dll"
                foreach ($dll in $dlls) {
                    Copy-Item $dll.FullName -Destination $BinDir -Force
                }
                Write-Host "[whisper.cpp] Installed to $whisperExe" -ForegroundColor Green
            } else {
                Write-Host "[whisper.cpp] Could not find executable in archive." -ForegroundColor Red
                Write-Host "  Contents:" -ForegroundColor Red
                Get-ChildItem -Path $extractDir -Recurse | ForEach-Object { Write-Host "    $($_.FullName)" }
            }

            # Cleanup
            Remove-Item -Force $zipPath -ErrorAction SilentlyContinue
            Remove-Item -Recurse -Force $extractDir -ErrorAction SilentlyContinue
        }
    }
}

# --- Step 2: Download ASR model ---
if (-not $SkipModel) {
    $modelFile = "ggml-$ModelSize.bin"
    $modelPath = Join-Path $ModelsDir $modelFile

    if (Test-Path $modelPath) {
        $size = (Get-Item $modelPath).Length / 1MB
        Write-Host "[Model] $modelFile already exists ($([math]::Round($size))MB)" -ForegroundColor Green
    } else {
        $modelUrl = "$HfRepo/$modelFile"
        Write-Host "[Model] Downloading $modelFile..." -ForegroundColor Yellow
        Write-Host "  URL: $modelUrl"
        Write-Host "  This may take a few minutes depending on your connection."

        try {
            # Use BITS for large file download with progress
            $progressPreference = 'Continue'
            Invoke-WebRequest -Uri $modelUrl -OutFile $modelPath -UseBasicParsing

            $size = (Get-Item $modelPath).Length / 1MB
            Write-Host "[Model] Downloaded $modelFile ($([math]::Round($size))MB)" -ForegroundColor Green
        } catch {
            Write-Host "[Model] Download failed: $_" -ForegroundColor Red
            Write-Host "  Download manually from: $modelUrl" -ForegroundColor Red
            Write-Host "  Place in: $ModelsDir" -ForegroundColor Red
        }
    }
}

# --- Step 3: Ollama setup ---
if (-not $SkipOllama) {
    $ollamaCmd = Get-Command ollama -ErrorAction SilentlyContinue

    if ($ollamaCmd) {
        Write-Host "[Ollama] Found at $($ollamaCmd.Source)" -ForegroundColor Green

        # Check if model is pulled
        Write-Host "[Ollama] Pulling qwen3.5:0.8b (if not already present)..." -ForegroundColor Yellow
        try {
            & ollama pull qwen3.5:0.8b
            Write-Host "[Ollama] Model qwen3.5:0.8b ready" -ForegroundColor Green
        } catch {
            Write-Host "[Ollama] Failed to pull model: $_" -ForegroundColor Red
        }
    } else {
        Write-Host "[Ollama] Not installed (optional - needed only for text cleanup)" -ForegroundColor Yellow
        Write-Host "  Install from: https://ollama.com/download" -ForegroundColor Yellow
        Write-Host "  Then run: ollama pull qwen3.5:0.8b" -ForegroundColor Yellow
    }
}

# --- Step 4: Write/update config ---
$configDir = Join-Path $env:APPDATA "unmute"
$configPath = Join-Path $configDir "config.json"

New-Item -ItemType Directory -Force -Path $configDir | Out-Null

$whisperExePath = Join-Path $BinDir "whisper-cli.exe"

if (Test-Path $configPath) {
    # Update existing config with paths
    $config = Get-Content $configPath | ConvertFrom-Json

    if ([string]::IsNullOrEmpty($config.whisper_path) -or -not (Test-Path $config.whisper_path)) {
        $config.whisper_path = $whisperExePath
    }
    if ([string]::IsNullOrEmpty($config.models_dir) -or -not (Test-Path $config.models_dir)) {
        $config.models_dir = $ModelsDir
    }

    $config | ConvertTo-Json -Depth 10 | Set-Content $configPath
    Write-Host "[Config] Updated $configPath" -ForegroundColor Green
} else {
    # Create new config
    $config = @{
        hotkey_hold = "Alt+Control"
        hotkey_toggle = "Alt+Shift+Control"
        asr_model = $ModelSize
        asr_language = "en"
        whisper_path = $whisperExePath
        models_dir = $ModelsDir
        cleanup_mode = "off"
        cleanup_device = "gpu"
        cleanup_model = "qwen3.5:0.8b"
        ollama_url = "http://localhost:11434"
        auto_paste = $true
        max_recording_secs = 120
    }

    $config | ConvertTo-Json -Depth 10 | Set-Content $configPath
    Write-Host "[Config] Created $configPath" -ForegroundColor Green
}

# --- Summary ---
Write-Host ""
Write-Host "=== Setup Complete ===" -ForegroundColor Cyan
Write-Host ""
Write-Host "Paths:" -ForegroundColor White
Write-Host "  whisper-cli:  $whisperExePath"
Write-Host "  Models dir:   $ModelsDir"
Write-Host "  Config:       $configPath"
Write-Host "  Logs:         $(Join-Path $AppDir 'logs')"
Write-Host ""

$ready = $true

if (-not (Test-Path $whisperExePath)) {
    Write-Host "[!] whisper-cli.exe not found - ASR will not work" -ForegroundColor Red
    $ready = $false
}

$modelPath = Join-Path $ModelsDir "ggml-$ModelSize.bin"
if (-not (Test-Path $modelPath)) {
    Write-Host "[!] Model ggml-$ModelSize.bin not found - ASR will not work" -ForegroundColor Red
    $ready = $false
}

if ($ready) {
    Write-Host "Ready to run Unmute!" -ForegroundColor Green
    Write-Host "  Run the app or use: npx tauri dev" -ForegroundColor White
} else {
    Write-Host "Some components are missing. See errors above." -ForegroundColor Yellow
}

Write-Host ""
