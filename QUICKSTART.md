# 🚀 Démarrage rapide - Compress Speak

## Installation en 3 étapes

### 1️⃣ Prérequis
- [Python 3.10+](https://www.python.org/downloads/)
- [FFmpeg](https://ffmpeg.org/download.html) (ou `winget install ffmpeg`)

### 2️⃣ Installation automatique
```powershell
.\install.ps1
```
Répondez **"o"** pour installer audio-separator (UVR5) et Vibe (transcription).

### 3️⃣ Lancement
```powershell
.\run.ps1
```

---

## Utilisation basique

### Conversion simple
1. Glissez-déposez vos fichiers audio
2. Ajustez le gain (18 dB par défaut)
3. (Optionnel) **Paramètres → Paramètres de conversion…** pour choisir un preset (Audiobook, Podcast, Voice Chat, Note vocale)
4. Cliquez **"Convertir en Opus"**

✅ Résultat : Fichiers `.opus` compressés dans le même dossier que les sources.

---

## Options avancées

### 🎤 Isolation vocale (UVR5)
Cochez **"Isoler voix"** sur chaque fichier pour extraire uniquement la voix.

**Configuration GPU (recommandé)** :
- **Paramètres → Configurer UVR5…**
- Sélectionnez votre carte graphique (ex: "1: AMD Radeon RX 6750 XT")

### 🔥 Chaleur vocale
Cochez **"Réchauffer la voix"** pour ajouter harmoniques et corps à la voix extraite.

**Configuration** :
- **Paramètres → Chaleur vocale (Pedalboard)…**
- Choisissez un preset (Subtil, Standard, Radio, Podcast) ou personnalisez

### 💾 Conserver FLAC
Cochez **"Conserver le fichier FLAC"** pour garder une version lossless.

### ✍️ Transcription
Cochez **"Transcrire"** sur chaque fichier pour générer un fichier texte.

**Configuration** :
- **Paramètres → Configurer Vibe…**
- Modèle : **small** (bon compromis) ou **medium** (meilleure qualité)
- Langue : **french** ou **auto**

---

## Workflow complet (toutes options)

```
Fichier audio (MP3/WAV)
    ↓
[1] UVR5 : Extraction voix → FLAC
    ↓
[2] Pedalboard : Chaleur vocale → FLAC (warm)
    ↓
[3] FFmpeg : Compression → OPUS
    ↓
[4] Vibe : Transcription → TXT
    ↓
Résultats :
  - fichier.opus (voix compressée)
  - fichier_(Vocals)_*.flac (optionnel si "Conserver FLAC" coché)
  - fichier.txt (transcription si "Transcrire" coché)
```

---

## Raccourcis clavier

| Touche | Action |
|--------|--------|
| **Ctrl+O** | Ajouter des fichiers |
| **Ctrl+D** | Choisir dossier de sortie |
| **Espace** | Lecture/Pause |
| **Échap** | Annuler la conversion |

---

## Problèmes courants

### FFmpeg introuvable
```powershell
winget install ffmpeg
```

### GPU pas utilisé
**Paramètres → Configurer UVR5… → GPU DirectML** : Sélectionnez votre carte dédiée (index 1).

### Vibe introuvable
- Installer : `pip install vibe-whisper`
- Ou télécharger [sona.exe](https://github.com/thewh1teagle/vibe/releases)

---

## Documentation complète

➡️ Consultez le [README.md](README.md) pour la documentation complète.

## Support

Ouvrez une issue sur GitHub pour toute question ou bug.
