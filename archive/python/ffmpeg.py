# -*- coding: utf-8 -*-
"""Construction des commandes FFmpeg et parsing de la progression."""

import re
import subprocess
from pathlib import Path
from typing import Any

from config import (
    DEFAULT_AFFTDN_NF,
    DEFAULT_COMP_ATTACK_MS,
    DEFAULT_COMP_RELEASE_MS,
    DEFAULT_COMP_RATIO,
    DEFAULT_COMP_THRESHOLD_DB,
    DEFAULT_OPUS_BITRATE,
    DEFAULT_OPUS_PREVIEW_BITRATE,
    FFMPEG_CMD,
)

def _default_conversion_params() -> dict[str, Any]:
    return {
        "afftdn_nf": DEFAULT_AFFTDN_NF,
        "comp_threshold_db": DEFAULT_COMP_THRESHOLD_DB,
        "comp_ratio": DEFAULT_COMP_RATIO,
        "comp_attack_ms": DEFAULT_COMP_ATTACK_MS,
        "comp_release_ms": DEFAULT_COMP_RELEASE_MS,
        "opus_bitrate": DEFAULT_OPUS_BITRATE,
        "opus_preview_bitrate": DEFAULT_OPUS_PREVIEW_BITRATE,
    }


def build_af_filter(gain_db: int, params: dict[str, Any] | None = None) -> str:
    """Construit la chaîne -af pour FFmpeg (afftdn optionnel + volume + acompressor)."""
    p = _default_conversion_params()
    if params:
        p.update(params)
    nf = p.get("afftdn_nf", DEFAULT_AFFTDN_NF)
    th = p.get("comp_threshold_db", DEFAULT_COMP_THRESHOLD_DB)
    ratio = p.get("comp_ratio", DEFAULT_COMP_RATIO)
    attack = p.get("comp_attack_ms", DEFAULT_COMP_ATTACK_MS)
    release = p.get("comp_release_ms", DEFAULT_COMP_RELEASE_MS)
    skip_afftdn = bool(p.get("skip_afftdn", False))
    tail = (
        f"volume={gain_db}dB,"
        f"acompressor=threshold={th}dB:ratio={ratio}:attack={attack}:release={release}"
    )
    if skip_afftdn:
        return tail
    return f"afftdn=nf={nf},{tail}"



def ffmpeg_available() -> bool:
    """Vérifie si ffmpeg est disponible dans le PATH."""
    try:
        subprocess.run([FFMPEG_CMD, "-version"], capture_output=True, check=True)
        return True
    except (subprocess.CalledProcessError, FileNotFoundError):
        return False


def build_m4a_to_wav_command(input_path: Path, output_wav_path: Path) -> list[str]:
    """Construit la commande FFmpeg pour convertir un fichier audio (m4a, mp4, etc.) en WAV.
    Utilisé avant UVR5 pour éviter un bug d'audio-separator qui encode en m4a vers un fichier
    temporaire sans extension, ce qui fait échouer le muxer FFmpeg."""
    return [
        FFMPEG_CMD,
        "-y",
        "-i",
        str(input_path),
        "-vn",
        "-acodec",
        "pcm_s16le",
        str(output_wav_path),
    ]


def build_convert_command(
    input_path: Path,
    output_path: Path,
    gain_db: int,
    conversion_params: dict[str, Any] | None = None,
) -> list[str]:
    """Construit la commande FFmpeg pour la conversion (nettoyage + gain + Opus)."""
    p = _default_conversion_params()
    if conversion_params:
        p.update(conversion_params)
    bitrate = p.get("opus_bitrate", DEFAULT_OPUS_BITRATE)
    return [
        FFMPEG_CMD,
        "-y",
        "-i",
        str(input_path),
        "-vn",
        "-af",
        build_af_filter(gain_db, conversion_params),
        "-c:a",
        "libopus",
        "-b:a",
        str(bitrate),
        "-vbr",
        "on",
        "-application",
        "voip",
        str(output_path),
    ]


def build_preview_command(
    input_path: Path,
    output_path: Path,
    start_sec: float,
    duration_sec: float,
    gain_db: int,
    conversion_params: dict[str, Any] | None = None,
) -> list[str]:
    """Construit la commande FFmpeg pour l'extrait de prévisualisation (5 s)."""
    p = _default_conversion_params()
    if conversion_params:
        p.update(conversion_params)
    bitrate = p.get("opus_preview_bitrate", DEFAULT_OPUS_PREVIEW_BITRATE)
    return [
        FFMPEG_CMD,
        "-y",
        "-ss",
        f"{start_sec:.3f}",
        "-i",
        str(input_path),
        "-t",
        f"{duration_sec:.3f}",
        "-vn",
        "-af",
        build_af_filter(gain_db, conversion_params),
        "-c:a",
        "libopus",
        "-b:a",
        str(bitrate),
        "-vbr",
        "on",
        "-application",
        "voip",
        str(output_path),
    ]


def parse_progress_from_output(text: str, duration_sec: float) -> int | None:
    """
    Extrait la progression en pourcentage (0-100) depuis une ligne de sortie FFmpeg (time=...).
    Retourne None si impossible à parser.
    """
    if not duration_sec or duration_sec <= 0:
        return None
    match = re.search(r"time=(\d+):(\d+):(\d+\.?\d*)", text)
    if not match:
        return None
    hours, minutes, seconds = match.groups()
    current_sec = int(hours) * 3600 + int(minutes) * 60 + float(seconds)
    return min(100, max(0, int((current_sec / duration_sec) * 100)))
