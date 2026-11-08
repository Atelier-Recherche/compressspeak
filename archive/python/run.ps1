# run.ps1
# Script de lancement pour Compress Speak

Write-Host "Lancement de Compress Speak..." -ForegroundColor Cyan

# Vérifier que l'environnement virtuel existe
if (-not (Test-Path ".venv")) {
    Write-Host "Erreur: Environnement virtuel non trouvé" -ForegroundColor Red
    Write-Host "Exécutez d'abord: .\install.ps1" -ForegroundColor Yellow
    exit 1
}

# Activer l'environnement et lancer
& ".\.venv\Scripts\Activate.ps1"
python app.py
