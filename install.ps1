# install.ps1
# Script d'installation automatique pour Compress Speak (Windows)

Write-Host "====================================" -ForegroundColor Cyan
Write-Host "   Compress Speak - Installation   " -ForegroundColor Cyan
Write-Host "====================================" -ForegroundColor Cyan
Write-Host ""

# 1. Vérifier Python
Write-Host "[1/6] Vérification de Python..." -ForegroundColor Yellow
try {
    $pythonVersion = python --version 2>&1
    if ($pythonVersion -match "Python (\d+)\.(\d+)") {
        $major = [int]$Matches[1]
        $minor = [int]$Matches[2]
        if ($major -ge 3 -and $minor -ge 10) {
            Write-Host "  ✓ Python $major.$minor détecté" -ForegroundColor Green
        } else {
            Write-Host "  ✗ Python 3.10+ requis (trouvé: $major.$minor)" -ForegroundColor Red
            Write-Host "    Téléchargez Python depuis https://www.python.org/downloads/" -ForegroundColor Yellow
            exit 1
        }
    }
} catch {
    Write-Host "  ✗ Python non trouvé" -ForegroundColor Red
    Write-Host "    Téléchargez Python depuis https://www.python.org/downloads/" -ForegroundColor Yellow
    exit 1
}

# 2. Vérifier FFmpeg
Write-Host "[2/6] Vérification de FFmpeg..." -ForegroundColor Yellow
try {
    $ffmpegVersion = ffmpeg -version 2>&1 | Select-String "ffmpeg version"
    Write-Host "  ✓ FFmpeg détecté" -ForegroundColor Green
} catch {
    Write-Host "  ⚠ FFmpeg non trouvé" -ForegroundColor Yellow
    Write-Host "    Installez FFmpeg avec: winget install ffmpeg" -ForegroundColor Yellow
    Write-Host "    Ou téléchargez depuis: https://ffmpeg.org/download.html" -ForegroundColor Yellow
    Write-Host "    L'installation continuera, mais FFmpeg est requis pour utiliser l'application." -ForegroundColor Yellow
}

# 3. Créer environnement virtuel
Write-Host "[3/6] Création de l'environnement virtuel..." -ForegroundColor Yellow
if (Test-Path ".venv") {
    Write-Host "  ⚠ .venv existe déjà, réutilisation" -ForegroundColor Yellow
} else {
    python -m venv .venv
    if ($LASTEXITCODE -eq 0) {
        Write-Host "  ✓ Environnement virtuel créé" -ForegroundColor Green
    } else {
        Write-Host "  ✗ Échec de création de l'environnement virtuel" -ForegroundColor Red
        exit 1
    }
}

# 4. Activer et installer dépendances de base
Write-Host "[4/6] Installation des dépendances de base..." -ForegroundColor Yellow
& ".\.venv\Scripts\Activate.ps1"
pip install -r requirements.txt --quiet
if ($LASTEXITCODE -eq 0) {
    Write-Host "  ✓ Dépendances de base installées (PyQt6, Pedalboard)" -ForegroundColor Green
} else {
    Write-Host "  ✗ Échec de l'installation des dépendances" -ForegroundColor Red
    exit 1
}

# 5. Proposer audio-separator (UVR5)
Write-Host "[5/6] Installation de audio-separator (UVR5)..." -ForegroundColor Yellow
$installUVR5 = Read-Host "  Installer audio-separator pour l'isolation vocale avec GPU AMD/Intel ? (o/N)"
if ($installUVR5 -eq "o" -or $installUVR5 -eq "O") {
    Write-Host "    Installation de audio-separator[gpu] + DirectML..." -ForegroundColor Cyan
    pip install audio-separator[gpu] onnxruntime-directml torch-directml --quiet
    if ($LASTEXITCODE -eq 0) {
        Write-Host "  ✓ audio-separator installé avec support DirectML (GPU)" -ForegroundColor Green
        
        # Tester DirectML
        Write-Host "    Test de DirectML..." -ForegroundColor Cyan
        python -c "import onnxruntime; providers = onnxruntime.get_available_providers(); print('DmlExecutionProvider disponible' if 'DmlExecutionProvider' in providers else 'DirectML non disponible (CPU seulement)')"
    } else {
        Write-Host "  ⚠ Échec partiel, installation CPU uniquement" -ForegroundColor Yellow
        pip install audio-separator --quiet
    }
} else {
    Write-Host "  ⊘ audio-separator non installé (vous pourrez l'installer plus tard)" -ForegroundColor Gray
}

# 6. Proposer Vibe (Whisper)
Write-Host "[6/6] Installation de Vibe (Whisper)..." -ForegroundColor Yellow
$installVibe = Read-Host "  Installer Vibe pour la transcription automatique ? (o/N)"
if ($installVibe -eq "o" -or $installVibe -eq "O") {
    Write-Host "    Quelle version préférez-vous ?" -ForegroundColor Cyan
    Write-Host "      1) vibe-whisper (pip, officiel)" -ForegroundColor White
    Write-Host "      2) sona.exe (téléchargement manuel, plus rapide)" -ForegroundColor White
    $vibeChoice = Read-Host "    Choix (1/2)"
    
    if ($vibeChoice -eq "1") {
        pip install vibe-whisper --quiet
        if ($LASTEXITCODE -eq 0) {
            Write-Host "  ✓ vibe-whisper installé" -ForegroundColor Green
        } else {
            Write-Host "  ⚠ Échec de l'installation de vibe-whisper" -ForegroundColor Yellow
        }
    } elseif ($vibeChoice -eq "2") {
        Write-Host "  → Téléchargez sona.exe depuis:" -ForegroundColor Yellow
        Write-Host "    https://github.com/thewh1teagle/vibe/releases" -ForegroundColor Cyan
        Write-Host "    Placez sona.exe dans le dossier compressspeak/" -ForegroundColor Yellow
    }
} else {
    Write-Host "  ⊘ Vibe non installé (vous pourrez l'installer plus tard)" -ForegroundColor Gray
}

Write-Host ""
Write-Host "====================================" -ForegroundColor Cyan
Write-Host "   Installation terminée !          " -ForegroundColor Green
Write-Host "====================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Pour lancer l'application:" -ForegroundColor White
Write-Host "  .\run.ps1" -ForegroundColor Cyan
Write-Host ""
Write-Host "Ou manuellement:" -ForegroundColor White
Write-Host "  .\.venv\Scripts\Activate.ps1" -ForegroundColor Cyan
Write-Host "  python app.py" -ForegroundColor Cyan
Write-Host ""
