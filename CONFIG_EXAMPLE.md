# Configuration avancée - Compress Speak

Ce guide explique comment personnaliser les paramètres de l'application via Python ou manuellement.

---

## 📍 Emplacement des paramètres

Les paramètres sont stockés dans le registre Windows (QSettings) :
- **Clé** : `HKEY_CURRENT_USER\Software\compressspeak\compressspeak`

---

## 🎛️ Modifier les paramètres via Python

### Script de configuration

Créez un fichier `configure.py` :

```python
from PyQt6.QtCore import QSettings

settings = QSettings("compressspeak", "compressspeak")

# === UVR5 (Isolation vocale) ===
settings.setValue("uvr5_enabled", True)
settings.setValue("uvr5_path", "")  # Vide = utiliser audio-separator
settings.setValue("uvr5_chunk_sec", 1200)  # 20 minutes
settings.setValue("uvr5_model", "MDX23C-8KFFT-InstVoc_HQ.ckpt")
settings.setValue("uvr5_mdx_segment_size", 2048)
settings.setValue("uvr5_mdxc_overlap", 8)
settings.setValue("uvr5_mdx_batch_size", 1)
settings.setValue("uvr5_directml_device", 1)  # 1 = GPU dédié, 0 = intégré, -1 = auto
settings.setValue("uvr5_stem", "Vocals")  # Vocals, Instrumental, Drums, Bass...

# === Warmth (Chaleur vocale) ===
settings.setValue("warmth_high_shelf_gain", 2.0)  # Gain médiums-graves (dB)
settings.setValue("warmth_high_shelf_cutoff", 400.0)  # Fréquence de coupure (Hz)
settings.setValue("warmth_distortion_drive", 3.0)  # Intensité distortion (dB)
settings.setValue("warmth_compressor_threshold", -15.0)  # Seuil compresseur (dB)
settings.setValue("warmth_compressor_ratio", 3.0)  # Ratio compression
settings.setValue("warmth_final_gain", 2.0)  # Gain final (dB)

# === Compression Opus ===
settings.setValue("opus_bitrate", "20k")  # Bitrate voix
settings.setValue("opus_preview_bitrate", "64k")  # Bitrate aperçu

# === Réduction de bruit ===
settings.setValue("afftdn_nf", -25)  # Noise floor (dB)

# === Compresseur dynamique ===
settings.setValue("comp_threshold", -21)  # Seuil (dB)
settings.setValue("comp_ratio", 2.5)  # Ratio
settings.setValue("comp_attack", 200)  # Attack (ms)
settings.setValue("comp_release", 1000)  # Release (ms)

# === Vibe (Transcription) ===
settings.setValue("vibe_path", "")  # Vide = auto-détection
settings.setValue("vibe_model", "small")  # tiny, base, small, medium, large
settings.setValue("vibe_language", "french")  # french, english, auto...
settings.setValue("vibe_format", "txt")  # txt, srt, vtt

# Sauvegarder
settings.sync()
print("Configuration sauvegardée !")
```

Exécuter :
```powershell
.\.venv\Scripts\Activate.ps1
python configure.py
```

---

## 🎨 Presets de chaleur vocale

### Preset "Subtil" (discret)
```python
settings.setValue("warmth_high_shelf_gain", 1.0)
settings.setValue("warmth_high_shelf_cutoff", 350.0)
settings.setValue("warmth_distortion_drive", 2.0)
settings.setValue("warmth_compressor_threshold", -18.0)
settings.setValue("warmth_compressor_ratio", 2.5)
settings.setValue("warmth_final_gain", 1.5)
```

### Preset "Standard" (défaut)
```python
settings.setValue("warmth_high_shelf_gain", 2.0)
settings.setValue("warmth_high_shelf_cutoff", 400.0)
settings.setValue("warmth_distortion_drive", 3.0)
settings.setValue("warmth_compressor_threshold", -15.0)
settings.setValue("warmth_compressor_ratio", 3.0)
settings.setValue("warmth_final_gain", 2.0)
```

### Preset "Radio" (punchy)
```python
settings.setValue("warmth_high_shelf_gain", 3.0)
settings.setValue("warmth_high_shelf_cutoff", 300.0)
settings.setValue("warmth_distortion_drive", 5.0)
settings.setValue("warmth_compressor_threshold", -20.0)
settings.setValue("warmth_compressor_ratio", 4.0)
settings.setValue("warmth_final_gain", 3.0)
```

### Preset "Podcast" (naturel avec corps)
```python
settings.setValue("warmth_high_shelf_gain", 1.5)
settings.setValue("warmth_high_shelf_cutoff", 450.0)
settings.setValue("warmth_distortion_drive", 2.5)
settings.setValue("warmth_compressor_threshold", -18.0)
settings.setValue("warmth_compressor_ratio", 2.0)
settings.setValue("warmth_final_gain", 1.5)
```

---

## 🎯 Presets de compression Opus

**Note** : Ces presets sont maintenant accessibles directement dans l'interface via **Paramètres → Paramètres de conversion (Opus)…**

### Preset "Audiobook" (qualité maximale)
```python
settings.setValue("opus_bitrate", "32k")
settings.setValue("opus_preview_bitrate", "64k")
settings.setValue("afftdn_nf", -25)
settings.setValue("comp_threshold", -18)
settings.setValue("comp_ratio", 2.0)
settings.setValue("comp_attack", 200)
settings.setValue("comp_release", 1000)
```

### Preset "Podcast" (standard - par défaut)
```python
settings.setValue("opus_bitrate", "20k")
settings.setValue("opus_preview_bitrate", "64k")
settings.setValue("afftdn_nf", -25)
settings.setValue("comp_threshold", -21)
settings.setValue("comp_ratio", 2.5)
settings.setValue("comp_attack", 200)
settings.setValue("comp_release", 1000)
```

### Preset "Voice Chat" (léger)
```python
settings.setValue("opus_bitrate", "16k")
settings.setValue("opus_preview_bitrate", "48k")
settings.setValue("afftdn_nf", -20)
settings.setValue("comp_threshold", -21)
settings.setValue("comp_ratio", 3.0)
settings.setValue("comp_attack", 150)
settings.setValue("comp_release", 800)
```

### Preset "Note vocale" (ultra-léger)
```python
settings.setValue("opus_bitrate", "12k")
settings.setValue("opus_preview_bitrate", "32k")
settings.setValue("afftdn_nf", -18)
settings.setValue("comp_threshold", -24)
settings.setValue("comp_ratio", 3.5)
settings.setValue("comp_attack", 100)
settings.setValue("comp_release", 500)
```

---

## 🔧 Personnalisation avancée

### Désactiver la réduction de bruit
```python
settings.setValue("afftdn_nf", -100)  # Valeur très basse = désactivé
```

### Compression plus agressive
```python
settings.setValue("comp_threshold", -24)  # Seuil plus bas
settings.setValue("comp_ratio", 4.0)  # Ratio plus élevé
settings.setValue("comp_attack", 100)  # Attack plus rapide
settings.setValue("comp_release", 500)  # Release plus rapide
```

### Priorité vitesse (UVR5)
```python
settings.setValue("uvr5_mdx_segment_size", 1024)  # Plus petit = plus rapide
settings.setValue("uvr5_mdx_batch_size", 4)  # Plus grand = plus rapide (+ VRAM)
settings.setValue("uvr5_chunk_sec", 1800)  # Chunks plus grands = moins de découpage
```

### Priorité qualité (UVR5)
```python
settings.setValue("uvr5_mdx_segment_size", 4096)  # Plus grand = meilleure qualité
settings.setValue("uvr5_mdx_batch_size", 1)  # Plus petit = moins de VRAM
settings.setValue("uvr5_mdxc_overlap", 16)  # Plus d'overlap = meilleure séparation
```

---

## 🔄 Réinitialiser la configuration

Pour revenir aux valeurs par défaut :

```python
from PyQt6.QtCore import QSettings

settings = QSettings("compressspeak", "compressspeak")
settings.clear()
settings.sync()
print("Configuration réinitialisée !")
```

---

## 📊 Lire la configuration actuelle

```python
from PyQt6.QtCore import QSettings

settings = QSettings("compressspeak", "compressspeak")

print("=== Configuration actuelle ===")
for key in settings.allKeys():
    print(f"{key}: {settings.value(key)}")
```

---

## 🎓 Valeurs recommandées par cas d'usage

### Podcast / Audiobook
- **Opus** : 24-32k
- **Chaleur** : Preset "Podcast"
- **UVR5** : Activé, stem "Vocals"
- **Transcription** : Model "medium", language "french"

### Note vocale / Mémo
- **Opus** : 12-16k
- **Chaleur** : Désactivée ou "Subtil"
- **UVR5** : Désactivé
- **Transcription** : Model "small", language "auto"

### Voiceover / Narration
- **Opus** : 32-48k
- **Chaleur** : Preset "Radio"
- **UVR5** : Activé pour isolation
- **Transcription** : Model "large", language spécifique

### Interview / Conférence
- **Opus** : 20-24k
- **Chaleur** : Preset "Standard"
- **UVR5** : Activé si bruit de fond
- **Transcription** : Model "medium", language "auto"

---

## 💡 Astuces

1. **Tester les presets** : Créez plusieurs fichiers de configuration et testez sur le même fichier audio
2. **Écouter avant/après** : Utilisez l'aperçu 5 secondes pour comparer
3. **Monitorer le GPU** : Gestionnaire des tâches → Performance → GPU pendant le traitement
4. **Batch processing** : Utilisez "Tout traiter" pour appliquer les mêmes paramètres à plusieurs fichiers

---

Pour toute question, consultez le [README.md](README.md) complet.
