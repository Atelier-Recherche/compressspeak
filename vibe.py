# -*- coding: utf-8 -*-
"""Helpers pour la transcription Vibe/sona (chemin, modèle, détection)."""

import sys
import subprocess
from pathlib import Path

from PyQt6 import QtCore

import i18n
from config import SETTINGS_VIBE_FORMAT, SETTINGS_VIBE_LANGUAGE, SETTINGS_VIBE_MODEL, SETTINGS_VIBE_PATH, VIBE_CMD, VIBE_FORMATS


def get_vibe_path_from_settings(settings: QtCore.QSettings, default_cmd: str = VIBE_CMD) -> str:
    """Retourne le chemin brut stocké dans les paramètres (ou défaut)."""
    settings.beginGroup(i18n.SETTINGS_GROUP)
    path = str(settings.value(SETTINGS_VIBE_PATH, "") or "").strip()
    settings.endGroup()
    return path if path else default_cmd


def resolve_vibe_exe(settings: QtCore.QSettings, default_cmd: str = VIBE_CMD) -> str:
    """
    Retourne l'exécutable à utiliser pour la transcription.
    Si l'utilisateur a défini vibe.exe, tente d'utiliser sona.exe dans le même dossier si disponible.
    """
    path = get_vibe_path_from_settings(settings, default_cmd)
    if not path or path == default_cmd:
        return default_cmd
    if path.lower().endswith("vibe.exe"):
        vibe_path = Path(path)
        if vibe_path.exists():
            sona_path = vibe_path.parent / "sona.exe"
            if sona_path.exists():
                try:
                    result = subprocess.run(
                        [str(sona_path), "transcribe", "--help"],
                        capture_output=True,
                        timeout=5,
                    )
                    if result.returncode == 0 or (result.stdout or result.stderr):
                        return str(sona_path)
                except Exception:
                    pass
    return path


def get_vibe_model(settings: QtCore.QSettings) -> str:
    settings.beginGroup(i18n.SETTINGS_GROUP)
    path = str(settings.value(SETTINGS_VIBE_MODEL, "") or "").strip()
    settings.endGroup()
    return path


def get_vibe_language(settings: QtCore.QSettings) -> str:
    settings.beginGroup(i18n.SETTINGS_GROUP)
    lang = str(settings.value(SETTINGS_VIBE_LANGUAGE, "") or "").strip()
    settings.endGroup()
    return lang


def get_vibe_format(settings: QtCore.QSettings) -> str:
    settings.beginGroup(i18n.SETTINGS_GROUP)
    fmt = str(settings.value(SETTINGS_VIBE_FORMAT, "txt") or "txt").strip().lower()
    settings.endGroup()
    return fmt if fmt in VIBE_FORMATS else "txt"


def list_vibe_models() -> list[str]:
    """Liste les modèles Whisper .bin dans le dossier de modèles Vibe (si trouvé)."""
    models = []
    try:
        if sys.platform == "win32":
            appdata = Path.home() / "AppData" / "Local" / "github.com.thewh1teagle.vibe"
        elif sys.platform == "darwin":
            appdata = Path.home() / "Library" / "Application Support" / "github.com.thewh1teagle.vibe"
        else:
            appdata = Path.home() / ".local" / "share" / "github.com.thewh1teagle.vibe"
        if appdata.exists():
            for f in appdata.rglob("*.bin"):
                models.append(str(f))
    except Exception:
        pass
    return models


def check_vibe_available(exe_path: str) -> bool:
    """Vérifie si l'exécutable de transcription répond à transcribe --help."""
    if not exe_path or not exe_path.strip():
        return False
    try:
        result = subprocess.run(
            [exe_path, "transcribe", "--help"],
            capture_output=True,
            timeout=8,
        )
        out = (result.stdout or b"") + (result.stderr or b"")
        text = out.decode(errors="ignore").lower()
        return "transcribe" in text or "usage" in text
    except Exception:
        return False


def check_sona_syntax(exe_path: str, log_callback=None) -> bool:
    """True si le CLI attend model puis audio en positionnel (ex. sona)."""
    if not exe_path or not exe_path.strip():
        return False
    try:
        result = subprocess.run(
            [exe_path, "transcribe", "--help"],
            capture_output=True,
            timeout=5,
        )
        out = (result.stdout or b"") + (result.stderr or b"")
        text = out.decode(errors="ignore")
        text_lower = text.lower()
        return ("<model" in text_lower and "<audio" in text_lower) or "<model.bin>" in text_lower
    except subprocess.TimeoutExpired:
        if log_callback:
            log_callback("ATTENTION: L'exécutable Vibe ne répond pas. Utilisez sona.exe pour la transcription CLI.")
        return False
    except Exception as e:
        if log_callback:
            log_callback(f"Erreur détection syntaxe: {type(e).__name__}: {e}")
        return False
