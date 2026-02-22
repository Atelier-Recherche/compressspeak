# Compress Speak

**Compress Speak** est une application de compression et traitement audio pour la voix, optimisée pour créer des fichiers Opus légers avec transcription automatique.

## 🎯 Fonctionnalités

- **Compression Opus** intelligente avec réduction de bruit (afftdn) et compression dynamique
- **Isolation vocale** (UVR5/audio-separator) avec accélération GPU AMD/Intel (DirectML)
- **Chaleur vocale** (Pedalboard) : ajoute harmoniques et corps à la voix extraite
- **Transcription automatique** (Vibe/Whisper) en français, anglais, allemand, espagnol, etc.
- **Traitement par lots** avec file d'attente
- **Lecteur audio** intégré avec aperçu 5 secondes
- **Interface moderne** : thèmes sombre/clair, multilingue (fr/en/de/es)

---

## 📦 Installation rapide (Windows)

### Prérequis
- **Windows 10/11**
- **Python 3.10+** ([télécharger ici](https://www.python.org/downloads/))
- **FFmpeg** ([télécharger ici](https://ffmpeg.org/download.html) ou installer via `winget install ffmpeg`)
- **GPU AMD ou Intel** (optionnel, pour accélération UVR5)

### Installation automatique

1. **Cloner ou télécharger** le dépôt :
   ```powershell
   git clone https://github.com/votre-repo/compressspeak.git
   cd compressspeak
   ```

2. **Exécuter le script d'installation** :
   ```powershell
   .\install.ps1
   ```
   
   Le script va :
   - Créer un environnement virtuel Python
   - Installer les dépendances de base (PyQt6, Pedalboard)
   - Proposer d'installer **audio-separator** (UVR5) avec DirectML
   - Proposer d'installer **Vibe** (transcription Whisper offline)

3. **Lancer l'application** :
   ```powershell
   .\run.ps1
   ```

---

## 🔧 Installation manuelle

### 1. Environnement Python

```powershell
# Créer un environnement virtuel
python -m venv .venv

# Activer l'environnement
.\.venv\Scripts\Activate.ps1

# Installer les dépendances de base
pip install -r requirements.txt
```

### 2. audio-separator (Isolation vocale UVR5)

**Pour GPU AMD/Intel (DirectML) :**
```powershell
pip install audio-separator[gpu]
pip install onnxruntime-directml
pip install torch-directml
```

**Pour CPU uniquement :**
```powershell
pip install audio-separator
```

**Vérification :**
```powershell
audio-separator --help
```

### 3. Vibe (Transcription Whisper)

**Option A : Installer Vibe officiel**
```powershell
pip install vibe-whisper
```

**Option B : Utiliser sona.exe (alternative plus rapide)**

1. Télécharger [sona.exe](https://github.com/thewh1teagle/vibe/releases) depuis les releases GitHub
2. Placer `sona.exe` dans le dossier `compressspeak/` ou dans votre PATH
3. L'application détectera automatiquement `sona.exe`

**Vérification :**
```powershell
# Pour Vibe
vibe --help

# Pour sona
.\sona.exe --help
```

---

## 🚀 Utilisation

### Lancement
```powershell
# Avec le script
.\run.ps1

# Ou directement
.\.venv\Scripts\Activate.ps1
python app.py
```

### Workflow de base

1. **Ajouter des fichiers** : Glissez-déposez des fichiers audio ou cliquez sur "Ajouter des fichiers…"
2. **Ajuster le gain** : Modifier le gain (en dB) pour chaque fichier
3. **Choisir le dossier de sortie** (optionnel) : Par défaut, les fichiers sont créés à côté des sources
4. **Convertir** : Cliquez sur "Convertir en Opus"

### Options avancées

#### 🎤 Isolation vocale (UVR5)
- Cochez **"Isoler voix"** sur chaque fichier pour extraire uniquement la voix
- Configure dans **Paramètres → Configurer UVR5…** :
  - Chemin d'installation UVR5 (laissez vide pour utiliser `audio-separator`)
  - GPU DirectML (sélectionnez votre carte graphique dédiée)
  - Stem à extraire (Vocals, Instrumental, Drums, etc.)
  - Chunk duration (20 min par défaut pour éviter découpage inutile)

#### 🔥 Chaleur vocale (Warmth)
- Cochez **"Réchauffer la voix"** pour ajouter harmoniques et corps après l'extraction
- Configure dans **Paramètres → Chaleur vocale (Pedalboard)…** :
  - **Presets disponibles** : Subtil, Standard, Radio (Punchy), Podcast (Naturel)
  - **HighShelfFilter** : Gain et fréquence de coupure (ajoute du corps)
  - **Distortion** : Drive (génère des harmoniques chauds)
  - **Compressor** : Seuil et ratio (lisse la dynamique)
  - **Gain final** : Compensation de volume

#### 💾 Conserver FLAC
- Cochez **"Conserver le fichier FLAC"** pour garder une version lossless de la voix isolée
- Utile pour retraiter ultérieurement sans repasser par UVR5

#### ✍️ Transcription (Vibe)
- Cochez **"Transcrire"** sur chaque fichier pour générer un fichier texte de transcription
- Configure dans **Paramètres → Configurer Vibe…** :
  - Chemin de `vibe` ou `sona.exe` (auto-détecté si dans le PATH)
  - Modèle Whisper (tiny, base, small, medium, large)
  - Langue (auto, french, english, etc.)
  - Format de sortie (txt, srt, vtt)

#### ⚙️ Paramètres de conversion
Configure dans **Paramètres → Paramètres de conversion (Opus)…** :
- **Presets disponibles** :
  - **Audiobook** : Qualité max (32k, compression douce)
  - **Podcast** : Standard (20k, équilibré) - *par défaut*
  - **Voice Chat** : Léger (16k, compression modérée)
  - **Note vocale** : Ultra-léger (12k, compression agressive)
- **Paramètres personnalisables** :
  - Réduction de bruit (afftdn)
  - Compresseur (seuil, ratio, attack, release)
  - Bitrate Opus (conversion + aperçu)

---

## 📁 Structure du projet

```
compressspeak/
├── app.py                  # Point d'entrée
├── main_window.py          # Interface principale
├── config.py               # Configuration et constantes
├── i18n.py                 # Traductions (fr/en/de/es)
├── ffmpeg.py               # Wrapper FFmpeg
├── uvr5.py                 # Intégration UVR5/audio-separator
├── uvr5_worker.py          # Worker process pour UVR5 (DirectML)
├── warmth.py               # Effets de chaleur vocale (Pedalboard)
├── vibe.py                 # Intégration Vibe/Whisper
├── widgets.py              # Widgets PyQt6 personnalisés
├── theme.py                # Gestion des thèmes
├── requirements.txt        # Dépendances Python
├── install.ps1             # Script d'installation Windows
├── run.ps1                 # Script de lancement
└── README.md               # Ce fichier
```

---

## 🎛️ Configuration avancée

### DirectML (GPU AMD/Intel)

Pour forcer l'utilisation de votre carte graphique dédiée :

1. Ouvrir **Paramètres → Configurer UVR5…**
2. Dans **"GPU DirectML"**, sélectionner votre carte (ex: "AMD Radeon RX 6750 XT")
3. Laisser "-1 (auto)" utilisera le GPU intégré par défaut

### Modèles UVR5

Les modèles sont téléchargés automatiquement par `audio-separator`. Pour utiliser un modèle local :

1. Télécharger les modèles depuis [UVR5 GUI](https://github.com/Anjok07/ultimatevocalremovergui/releases)
2. Placer dans `C:\Users\<USER>\AppData\Local\Programs\Ultimate Vocal Remover\models\MDX_Net_Models\`
3. Spécifier le nom du modèle dans **Paramètres → Configurer UVR5… → Nom du modèle**

Modèles recommandés :
- **MDX23C-8KFFT-InstVoc_HQ.ckpt** (par défaut, qualité maximale)
- **UVR-MDX-NET-Inst_HQ_3.onnx** (plus rapide)

### Personnaliser la chaleur vocale

Les paramètres Warmth sont stockés dans `QSettings`. Pour modifier :

```python
from PyQt6.QtCore import QSettings
settings = QSettings("compressspeak", "compressspeak")

settings.setValue("warmth_high_shelf_gain", 3.0)        # Gain (dB)
settings.setValue("warmth_high_shelf_cutoff", 350.0)   # Fréquence (Hz)
settings.setValue("warmth_distortion_drive", 4.0)      # Drive (dB)
settings.setValue("warmth_compressor_threshold", -18.0) # Seuil (dB)
settings.setValue("warmth_compressor_ratio", 4.0)      # Ratio
settings.setValue("warmth_final_gain", 3.0)            # Gain final (dB)
```

---

## 🐛 Dépannage

### FFmpeg introuvable
**Erreur** : `FFmpeg introuvable`

**Solution** :
```powershell
# Vérifier l'installation
ffmpeg -version

# Installer via winget (Windows 10+)
winget install ffmpeg

# Ou ajouter FFmpeg au PATH manuellement
```

### DirectML non détecté
**Erreur** : `No hardware acceleration could be configured`

**Solutions** :
1. Vérifier les drivers GPU AMD/Intel à jour
2. Réinstaller `onnxruntime-directml` et `torch-directml` :
   ```powershell
   pip uninstall onnxruntime-directml torch-directml
   pip install onnxruntime-directml torch-directml
   ```
3. Vérifier l'adaptateur DirectML :
   ```powershell
   python -c "import onnxruntime; print(onnxruntime.get_available_providers())"
   # Doit afficher: ['DmlExecutionProvider', 'CPUExecutionProvider']
   ```

### GPU intégré utilisé au lieu de la carte dédiée
**Problème** : Le GPU intégré (adapter 0) est utilisé au lieu de la RX 6750 XT (adapter 1)

**Solution** :
1. **Paramètres → Configurer UVR5…**
2. Sélectionner **"1: AMD Radeon RX 6750 XT"** dans le menu déroulant "GPU DirectML"
3. Cliquer OK

### Vibe/sona introuvable
**Erreur** : `Vibe non installé`

**Solution** :
1. Installer Vibe : `pip install vibe-whisper`
2. Ou télécharger `sona.exe` et spécifier le chemin dans **Paramètres → Configurer Vibe…**

### UI freeze pendant le traitement
**Problème** : L'interface se fige pendant UVR5

**Solution** : Ce problème a été corrigé dans les versions récentes. UVR5 tourne maintenant dans un processus séparé (`uvr5_worker.py`). Si le problème persiste :
- Mettre à jour vers la dernière version
- Vérifier que `uvr5_worker.py` est présent dans le dossier

### Chunking bloqué sur fichiers de 15 min
**Problème** : Le traitement semble bloqué sur "File duration 900.0s exceeds chunk size"

**Solution** : Augmenter `chunk_duration` dans **Paramètres → Configurer UVR5…** à **1200 secondes** (20 min). La migration est automatique depuis les versions récentes.

---

## 🌍 Contribuer

Les contributions sont les bienvenues ! Pour ajouter une fonctionnalité ou corriger un bug :

1. Fork le projet
2. Créer une branche : `git checkout -b feature/ma-feature`
3. Commit : `git commit -m "Ajoute ma feature"`
4. Push : `git push origin feature/ma-feature`
5. Ouvrir une Pull Request

### Ajouter une traduction

Les traductions sont dans `i18n.py`. Pour ajouter une langue :

```python
TRANSLATIONS: dict[str, dict[str, str]] = {
    "app.title": {
        "fr": "Compress Speak",
        "en": "Compress Speak",
        "de": "Compress Speak",
        "es": "Compress Speak",
        "pt": "Compress Speak",  # Nouvelle langue
    },
    # ... autres clés
}
```

---

## 📄 Licence

[À définir]

---

## 🙏 Remerciements

- **audio-separator** : [karaokenerds/python-audio-separator](https://github.com/karaokenerds/python-audio-separator)
- **UVR5** : [Anjok07/ultimatevocalremovergui](https://github.com/Anjok07/ultimatevocalremovergui)
- **Vibe** : [thewh1teagle/vibe](https://github.com/thewh1teagle/vibe)
- **Pedalboard** : [spotify/pedalboard](https://github.com/spotify/pedalboard)
- **PyQt6** : [Riverbank Computing](https://www.riverbankcomputing.com/software/pyqt/)

---

## 📧 Contact

Pour toute question ou suggestion, ouvrez une issue sur GitHub.
