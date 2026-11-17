# -*- coding: utf-8 -*-
"""Constantes et utilitaires partagés pour Compress Speak."""

from pathlib import Path

FFMPEG_CMD = "ffmpeg"
VIBE_CMD = "vibe"
SETTINGS_VIBE_PATH = "vibe_path"
SETTINGS_VIBE_MODEL = "vibe_model"
SETTINGS_VIBE_LANGUAGE = "vibe_language"
SETTINGS_VIBE_FORMAT = "vibe_format"
VIBE_FORMATS = ("txt", "srt", "vtt")
VIBE_LANGUAGES = (
    ("", "auto"),
    ("french", "fr"),
    ("english", "en"),
    ("german", "de"),
    ("spanish", "es"),
    ("italian", "it"),
)
DEFAULT_GAIN_DB = 18
MIN_GAIN_DB = -30
MAX_GAIN_DB = 60

# Paramètres de conversion (compression / effets) — clés QSettings
SETTINGS_AFFTDN_NF = "afftdn_nf"
SETTINGS_COMP_THRESHOLD = "comp_threshold"
SETTINGS_COMP_RATIO = "comp_ratio"
SETTINGS_COMP_ATTACK = "comp_attack"
SETTINGS_COMP_RELEASE = "comp_release"
SETTINGS_OPUS_BITRATE = "opus_bitrate"
SETTINGS_OPUS_PREVIEW_BITRATE = "opus_preview_bitrate"

# Valeurs par défaut conversion
DEFAULT_AFFTDN_NF = -25
DEFAULT_COMP_THRESHOLD_DB = -21
DEFAULT_COMP_RATIO = 2.5
DEFAULT_COMP_ATTACK_MS = 200
DEFAULT_COMP_RELEASE_MS = 1000
DEFAULT_OPUS_BITRATE = "20k"
DEFAULT_OPUS_PREVIEW_BITRATE = "64k"

# UVR5 / audio-separator
SETTINGS_UVR5_ENABLED = "uvr5_enabled"
SETTINGS_UVR5_PATH = "uvr5_path"
SETTINGS_UVR5_CHUNK_SEC = "uvr5_chunk_sec"
SETTINGS_UVR5_MODEL = "uvr5_model"
SETTINGS_UVR5_MDX_SEGMENT_SIZE = "uvr5_mdx_segment_size"
SETTINGS_UVR5_MDXC_OVERLAP = "uvr5_mdxc_overlap"
SETTINGS_UVR5_MDX_BATCH_SIZE = "uvr5_mdx_batch_size"
SETTINGS_UVR5_DIRECTML_DEVICE = "uvr5_directml_device"  # Index de l'adapteur DirectML (-1 = auto, 0+ = index spécifique)
SETTINGS_UVR5_STEM = "uvr5_stem"  # Stem à extraire (Vocals, Instrumental, Drums, Bass, etc.)
UVR5_CHUNK_DURATION_DEFAULT_SEC = 1200  # 20 min (marge pour éviter chunking inutile sur fichiers de 15-18 min)
UVR5_DIRECTML_DEVICE_DEFAULT = 1  # 1 = généralement la carte graphique dédiée (0 = GPU intégré)
UVR5_STEM_DEFAULT = "Vocals"  # Par défaut, extraire les voix
UVR5_AVAILABLE_STEMS = ["Vocals", "Instrumental", "Drums", "Bass", "Guitar", "Piano", "Other"]
AUDIO_SEPARATOR_CMD = "audio-separator"
# Valeurs par défaut (audio-separator) ; 2048 souvent utilisé pour MDX23 / qualité
UVR5_MDX_SEGMENT_SIZE_DEFAULT = 2048
UVR5_MDXC_OVERLAP_DEFAULT = 8
UVR5_MDX_BATCH_SIZE_DEFAULT = 1

AUDIO_EXTENSIONS = {".wav", ".mp3", ".flac", ".aac", ".ogg", ".opus", ".m4a", ".mp4"}

# Formulaire principal (options conservées au redémarrage)
SETTINGS_FORM_CLEAN_SUFFIX = "form_clean_suffix"
SETTINGS_FORM_KEEP_FLAC = "form_keep_flac"
SETTINGS_OUTPUT_DIR = "output_dir"

# Warmth (chaleur vocale avec Pedalboard)
SETTINGS_WARMTH_ENABLED = "warmth_enabled"
SETTINGS_WARMTH_HIGH_SHELF_GAIN = "warmth_high_shelf_gain"
SETTINGS_WARMTH_HIGH_SHELF_CUTOFF = "warmth_high_shelf_cutoff"
SETTINGS_WARMTH_DISTORTION_DRIVE = "warmth_distortion_drive"
SETTINGS_WARMTH_COMPRESSOR_THRESHOLD = "warmth_compressor_threshold"
SETTINGS_WARMTH_COMPRESSOR_RATIO = "warmth_compressor_ratio"
SETTINGS_WARMTH_FINAL_GAIN = "warmth_final_gain"

# Valeurs par défaut Warmth
DEFAULT_WARMTH_HIGH_SHELF_GAIN_DB = 2.0
DEFAULT_WARMTH_HIGH_SHELF_CUTOFF_HZ = 400.0
DEFAULT_WARMTH_DISTORTION_DRIVE_DB = 3.0
DEFAULT_WARMTH_COMPRESSOR_THRESHOLD_DB = -15.0
DEFAULT_WARMTH_COMPRESSOR_RATIO = 3.0
DEFAULT_WARMTH_FINAL_GAIN_DB = 2.0

# DPDF / dpdfnet (débruitage léger ONNX CPU)
SETTINGS_DPDF_MODEL = "dpdf_model"
SETTINGS_DPDF_ATTN_LIMIT_DB = "dpdf_attn_limit_db"
DEFAULT_DPDF_MODEL = "dpdfnet2"
DEFAULT_DPDF_ATTN_LIMIT_DB = 0.0  # 0 = désactivé (défaut modèle)


def is_audio(path: Path) -> bool:
    """Retourne True si le fichier est un fichier audio reconnu."""
    return path.suffix.lower() in AUDIO_EXTENSIONS
