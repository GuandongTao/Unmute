# Unmute Setup Script
# Downloads whisper.cpp (CPU + CUDA), ASR model, and optionally sets up Ollama
# Run: powershell -ExecutionPolicy Bypass -File scripts\setup.ps1

param(
    [switch]$SkipWhisper,
    [switch]$SkipModel,
    [switch]$SkipOllama,
    [switch]$SkipCuda,
    [string]$ModelSize = "small.en"
)

$ErrorActionPreference = "Stop"

$AppDir = Join-Path $env:LOCALAPPDATA "unmute"
$BinDir = Join-Path $AppDir "bin"
$GpuBinDir = Join-Path $AppDir "bin-gpu"
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
New-Item -ItemType Directory -Force -Path $GpuBinDir | Out-Null
New-Item -ItemType Directory -Force -Path $ModelsDir | Out-Null

# --- Helper function to download and extract whisper build ---
function Install-WhisperBuild {
    param(
        [string]$ZipName,
        [string]$DestDir,
        [string]$Label
    )
    $exePath = Join-Path $DestDir "whisper-cli.exe"
    if (Test-Path $exePath) {
        Write-Host "[whisper.cpp $Label] Already installed at $exePath" -ForegroundColor Green
        return $true
    }

    $downloadUrl = "https://github.com/$WhisperRepo/releases/download/$WhisperVersion/$ZipName"
    $zipPath = Join-Path $env:TEMP "whisper-$Label.zip"

    Write-Host "[whisper.cpp $Label] Downloading $ZipName..." -ForegroundColor Yellow
    Write-Host "  URL: $downloadUrl"

    try {
        Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath -UseBasicParsing
    } catch {
        Write-Host "[whisper.cpp $Label] Download failed: $_" -ForegroundColor Red
        return $false
    }

    Write-Host "[whisper.cpp $Label] Extracting..." -ForegroundColor Yellow
    $extractDir = Join-Path $env:TEMP "whisper-extract-$Label"
    if (Test-Path $extractDir) { Remove-Item -Recurse -Force $extractDir }
    Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

    $exeFiles = Get-ChildItem -Path $extractDir -Recurse -Filter "whisper-cli.exe"
    if ($exeFiles.Count -eq 0) {
        $exeFiles = Get-ChildItem -Path $extractDir -Recurse -Filter "main.exe"
    }

    if ($exeFiles.Count -gt 0) {
        Copy-Item $exeFiles[0].FullName -Destination $exePath -Force
        # Copy DLLs (needed for CUDA builds)
        $dlls = Get-ChildItem -Path $exeFiles[0].DirectoryName -Filter "*.dll"
        foreach ($dll in $dlls) {
            Copy-Item $dll.FullName -Destination $DestDir -Force
        }
        Write-Host "[whisper.cpp $Label] Installed to $exePath" -ForegroundColor Green
    } else {
        Write-Host "[whisper.cpp $Label] Could not find executable in archive." -ForegroundColor Red
    }

    Remove-Item -Force $zipPath -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $extractDir -ErrorAction SilentlyContinue
    return (Test-Path $exePath)
}

# --- Step 1: Download whisper.cpp CPU build ---
$cpuInstalled = $false
if (-not $SkipWhisper) {
    $cpuInstalled = Install-WhisperBuild -ZipName "whisper-bin-x64.zip" -DestDir $BinDir -Label "CPU"
}

# --- Step 1b: Download whisper.cpp CUDA build (optional) ---
$cudaInstalled = $false
if (-not $SkipWhisper -and -not $SkipCuda) {
    Write-Host ""
    Write-Host "[whisper.cpp CUDA] Attempting CUDA/GPU build download..." -ForegroundColor Yellow
    Write-Host "  Note: Requires NVIDIA GPU with CUDA support. AMD GPUs are not supported." -ForegroundColor DarkGray
    $cudaInstalled = Install-WhisperBuild -ZipName "whisper-cublas-12.4.0-bin-x64.zip" -DestDir $GpuBinDir -Label "CUDA"
    if (-not $cudaInstalled) {
        Write-Host "[whisper.cpp CUDA] CUDA build not available. ASR will use CPU only." -ForegroundColor Yellow
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
        Write-Host "[Ollama] Pulling qwen2.5:3b (if not already present)..." -ForegroundColor Yellow
        try {
            & ollama pull qwen2.5:3b
            Write-Host "[Ollama] Model qwen2.5:3b ready" -ForegroundColor Green
        } catch {
            Write-Host "[Ollama] Failed to pull model: $_" -ForegroundColor Red
        }
    } else {
        Write-Host "[Ollama] Not installed (optional - needed only for text cleanup)" -ForegroundColor Yellow
        Write-Host "  Install from: https://ollama.com/download" -ForegroundColor Yellow
        Write-Host "  Then run: ollama pull qwen2.5:3b" -ForegroundColor Yellow
    }
}

# --- Step 4: Write/update config ---
$configDir = Join-Path $env:APPDATA "unmute"
$configPath = Join-Path $configDir "config.json"

New-Item -ItemType Directory -Force -Path $configDir | Out-Null

$whisperExePath = Join-Path $BinDir "whisper-cli.exe"
$whisperGpuExePath = Join-Path $GpuBinDir "whisper-cli.exe"
$defaultDevice = if ($cudaInstalled) { "gpu" } else { "cpu" }

if (Test-Path $configPath) {
    $config = Get-Content $configPath | ConvertFrom-Json

    if ([string]::IsNullOrEmpty($config.whisper_path) -or -not (Test-Path $config.whisper_path)) {
        $config.whisper_path = $whisperExePath
    }
    if (-not (Get-Member -InputObject $config -Name "whisper_gpu_path" -MemberType Properties)) {
        $config | Add-Member -NotePropertyName "whisper_gpu_path" -NotePropertyValue ""
    }
    if ($cudaInstalled -and ([string]::IsNullOrEmpty($config.whisper_gpu_path) -or -not (Test-Path $config.whisper_gpu_path))) {
        $config.whisper_gpu_path = $whisperGpuExePath
    }
    if (-not (Get-Member -InputObject $config -Name "asr_device" -MemberType Properties)) {
        $config | Add-Member -NotePropertyName "asr_device" -NotePropertyValue $defaultDevice
    }
    if ([string]::IsNullOrEmpty($config.models_dir) -or -not (Test-Path $config.models_dir)) {
        $config.models_dir = $ModelsDir
    }

    $config | ConvertTo-Json -Depth 10 | Set-Content $configPath
    Write-Host "[Config] Updated $configPath" -ForegroundColor Green
} else {
    $config = @{
        asr_model = $ModelSize
        asr_language = "en"
        asr_device = $defaultDevice
        whisper_path = $whisperExePath
        whisper_gpu_path = if ($cudaInstalled) { $whisperGpuExePath } else { "" }
        models_dir = $ModelsDir
        cleanup_mode = "off"
        cleanup_device = "gpu"
        cleanup_model = "qwen2.5:3b"
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
Write-Host "  whisper-cli (CPU):  $whisperExePath"
if ($cudaInstalled) {
    Write-Host "  whisper-cli (GPU):  $whisperGpuExePath"
}
Write-Host "  Models dir:         $ModelsDir"
Write-Host "  Config:             $configPath"
Write-Host ""

$ready = $true

if (-not (Test-Path $whisperExePath)) {
    Write-Host "[!] whisper-cli.exe (CPU) not found - ASR will not work" -ForegroundColor Red
    $ready = $false
}

$modelPath = Join-Path $ModelsDir "ggml-$ModelSize.bin"
if (-not (Test-Path $modelPath)) {
    Write-Host "[!] Model ggml-$ModelSize.bin not found - ASR will not work" -ForegroundColor Red
    $ready = $false
}

if ($ready) {
    Write-Host "Ready to run Unmute!" -ForegroundColor Green
    if ($cudaInstalled) {
        Write-Host "  GPU acceleration enabled (NVIDIA CUDA)" -ForegroundColor Green
    } else {
        Write-Host "  Running in CPU-only mode" -ForegroundColor Yellow
    }
    Write-Host "  Run the app or use: npx tauri dev" -ForegroundColor White
} else {
    Write-Host "Some components are missing. See errors above." -ForegroundColor Yellow
}

Write-Host ""
