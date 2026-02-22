# -*- coding: utf-8 -*-
"""Intégration UVR5 / audio-separator pour nettoyage de la voix (extraction Vocals)."""

import logging
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import i18n
from config import (
    AUDIO_SEPARATOR_CMD,
    SETTINGS_UVR5_CHUNK_SEC,
    SETTINGS_UVR5_MODEL,
    SETTINGS_UVR5_PATH,
    SETTINGS_UVR5_MDX_SEGMENT_SIZE,
    SETTINGS_UVR5_MDXC_OVERLAP,
    SETTINGS_UVR5_MDX_BATCH_SIZE,
    SETTINGS_UVR5_DIRECTML_DEVICE,
    SETTINGS_UVR5_STEM,
    UVR5_CHUNK_DURATION_DEFAULT_SEC,
    UVR5_MDX_SEGMENT_SIZE_DEFAULT,
    UVR5_MDXC_OVERLAP_DEFAULT,
    UVR5_MDX_BATCH_SIZE_DEFAULT,
    UVR5_DIRECTML_DEVICE_DEFAULT,
    UVR5_STEM_DEFAULT,
    UVR5_AVAILABLE_STEMS,
)

_IS_WIN = os.name == "nt"


def get_uvr5_path(settings) -> str:
    """Retourne le chemin configuré : dossier d'installation UVR5, script .py, exécutable, ou vide (= PATH)."""
    settings.beginGroup(i18n.SETTINGS_GROUP)
    path = str(settings.value(SETTINGS_UVR5_PATH, "") or "").strip()
    settings.endGroup()
    return path if path else AUDIO_SEPARATOR_CMD


def resolve_uvr5_command(uvr_path: str) -> list[str] | None:
    """
    Détermine la commande à lancer pour l'extraction Vocals (comme pour Vibe).
    - Chemin vide ou "audio-separator" → utilise la commande dans le PATH.
    - Fichier .py → python script.py
    - Fichier exe/autre → exécutable directement.
    - Dossier (installation UVR5) → cherche dans l'ordre :
      venv/Scripts/audio-separator.exe, venv/bin/audio-separator,
      venv/Scripts/python.exe -m audio_separator, venv/bin/python -m audio_separator.
    Retourne la liste [exe] ou [python, "-m", "audio_separator"] ou [python, "script.py"], ou None.
    """
    if not uvr_path or uvr_path.strip() == "":
        exe = shutil.which(AUDIO_SEPARATOR_CMD) or AUDIO_SEPARATOR_CMD
        return [exe]
    uvr_path = uvr_path.strip()
    p = Path(uvr_path)
    if p.is_file():
        if p.suffix.lower() == ".py":
            python = shutil.which("python") or "python"
            return [python, str(p)]
        return [str(p)]
    if p.is_dir():
        # Dossier d'installation UVR5 : chercher audio-separator ou python du venv
        candidates = []
        if _IS_WIN:
            candidates = [
                p / "venv" / "Scripts" / "audio-separator.exe",
                p / ".venv" / "Scripts" / "audio-separator.exe",
                p / "Scripts" / "audio-separator.exe",
                p / "audio-separator.exe",
                p / "venv" / "Scripts" / "python.exe",
                p / ".venv" / "Scripts" / "python.exe",
            ]
        else:
            candidates = [
                p / "venv" / "bin" / "audio-separator",
                p / ".venv" / "bin" / "audio-separator",
                p / "bin" / "audio-separator",
                p / "venv" / "bin" / "python",
                p / ".venv" / "bin" / "python",
            ]
        for candidate in candidates:
            if candidate.exists():
                if "audio-separator" in candidate.name:
                    return [str(candidate)]
                # Python du venv → lancer -m audio_separator
                return [str(candidate), "-m", "audio_separator"]
        # Installation packagée type "Ultimate Vocal Remover" (UVR.exe) = GUI seule, pas de CLI
        if (p / "UVR.exe").exists() or (p / "UVR_Launcher.exe").exists():
            return None
        return None
    # Chemin invalide ou exe nommé autrement
    if uvr_path == AUDIO_SEPARATOR_CMD:
        exe = shutil.which(AUDIO_SEPARATOR_CMD) or AUDIO_SEPARATOR_CMD
        return [exe]
    return [uvr_path]


def get_uvr5_chunk_sec(settings) -> int:
    settings.beginGroup(i18n.SETTINGS_GROUP)
    val = settings.value(SETTINGS_UVR5_CHUNK_SEC, UVR5_CHUNK_DURATION_DEFAULT_SEC)
    settings.endGroup()
    try:
        chunk = max(60, min(3600, int(val)))
        # Migration automatique : si chunk=900, passer à 1200 pour éviter chunking sur fichiers de 15 min
        if chunk == 900:
            chunk = 1200
            settings.beginGroup(i18n.SETTINGS_GROUP)
            settings.setValue(SETTINGS_UVR5_CHUNK_SEC, 1200)
            settings.endGroup()
        return chunk
    except (TypeError, ValueError):
        return UVR5_CHUNK_DURATION_DEFAULT_SEC


def is_uvr_gui_install_folder(uvr_path: str) -> bool:
    """True si le chemin pointe vers l'installation packagée UVR (UVR.exe), qui n'a pas de mode CLI."""
    if not uvr_path or not uvr_path.strip():
        return False
    p = Path(uvr_path.strip())
    return p.is_dir() and ((p / "UVR.exe").exists() or (p / "UVR_Launcher.exe").exists())


def get_uvr5_model(settings) -> str:
    settings.beginGroup(i18n.SETTINGS_GROUP)
    m = str(settings.value(SETTINGS_UVR5_MODEL, "") or "").strip()
    settings.endGroup()
    return m


def get_uvr5_mdx_segment_size(settings) -> int:
    settings.beginGroup(i18n.SETTINGS_GROUP)
    v = settings.value(SETTINGS_UVR5_MDX_SEGMENT_SIZE, UVR5_MDX_SEGMENT_SIZE_DEFAULT)
    settings.endGroup()
    try:
        return max(256, min(4096, int(v)))
    except (TypeError, ValueError):
        return UVR5_MDX_SEGMENT_SIZE_DEFAULT


def get_uvr5_mdxc_overlap(settings) -> int:
    settings.beginGroup(i18n.SETTINGS_GROUP)
    v = settings.value(SETTINGS_UVR5_MDXC_OVERLAP, UVR5_MDXC_OVERLAP_DEFAULT)
    settings.endGroup()
    try:
        return max(2, min(50, int(v)))
    except (TypeError, ValueError):
        return UVR5_MDXC_OVERLAP_DEFAULT


def get_uvr5_mdx_batch_size(settings) -> int:
    settings.beginGroup(i18n.SETTINGS_GROUP)
    v = settings.value(SETTINGS_UVR5_MDX_BATCH_SIZE, UVR5_MDX_BATCH_SIZE_DEFAULT)
    settings.endGroup()
    try:
        return max(1, min(32, int(v)))
    except (TypeError, ValueError):
        return UVR5_MDX_BATCH_SIZE_DEFAULT


def get_uvr5_directml_device(settings) -> int:
    """Retourne l'index de l'adapteur DirectML à utiliser (-1 = auto)."""
    settings.beginGroup(i18n.SETTINGS_GROUP)
    v = settings.value(SETTINGS_UVR5_DIRECTML_DEVICE, UVR5_DIRECTML_DEVICE_DEFAULT)
    settings.endGroup()
    try:
        return max(-1, min(10, int(v)))  # -1 à 10 adapteurs max
    except (TypeError, ValueError):
        return UVR5_DIRECTML_DEVICE_DEFAULT


def get_uvr5_stem(settings) -> str:
    """Retourne le stem à extraire (Vocals, Instrumental, etc.)."""
    settings.beginGroup(i18n.SETTINGS_GROUP)
    stem = str(settings.value(SETTINGS_UVR5_STEM, UVR5_STEM_DEFAULT) or "").strip()
    settings.endGroup()
    if stem and stem in UVR5_AVAILABLE_STEMS:
        return stem
    return UVR5_STEM_DEFAULT


def is_uvr5_default_path(uvr_path: str) -> bool:
    """True si le chemin UVR est vide ou la commande par défaut (PATH). Permet d'utiliser l'API in-process avec DirectML."""
    if not uvr_path or not uvr_path.strip():
        return True
    return uvr_path.strip() == AUDIO_SEPARATOR_CMD


def uvr5_available(uvr_path: str) -> bool:
    """Vérifie si audio-separator (ou l'exécutable / dossier UVR5 configuré) est disponible."""
    cmd = resolve_uvr5_command(uvr_path or "")
    if not cmd:
        return False
    try:
        result = subprocess.run(
            cmd + ["--help"],
            capture_output=True,
            timeout=15,
        )
        out = (result.stdout or b"") + (result.stderr or b"")
        return b"separate" in out.lower() or b"audio" in out.lower() or b"--single_stem" in out.lower()
    except Exception:
        return False


def build_uvr5_command(
    uvr_path: str,
    input_path: Path,
    output_dir: Path,
    chunk_duration_sec: int,
    model: str = "",
    mdx_segment_size: int | None = None,
    mdxc_overlap: int | None = None,
    mdx_batch_size: int | None = None,
    stem: str = "Vocals",
) -> list[str] | None:
    """Construit la liste de commande pour lancer l'extraction du stem spécifié."""
    cmd = resolve_uvr5_command(uvr_path or "")
    if not cmd:
        return None
    cmd.extend([str(input_path)])
    cmd.extend(["--single_stem", stem])
    cmd.extend(["--output_dir", str(output_dir)])
    cmd.extend(["--chunk_duration", str(chunk_duration_sec)])
    if model:
        model_dir, model_name = _parse_model_path(model)
        if model_dir:
            cmd.extend(["--model_file_dir", model_dir])
        if model_name:
            cmd.extend(["--model_filename", model_name])
    if mdx_segment_size is not None:
        cmd.extend(["--mdx_segment_size", str(mdx_segment_size)])
    if mdxc_overlap is not None:
        cmd.extend(["--mdxc_overlap", str(mdxc_overlap)])
    if mdx_batch_size is not None:
        cmd.extend(["--mdx_batch_size", str(mdx_batch_size)])
    return cmd


def run_uvr5_vocals(
    input_path: Path,
    output_dir: Path,
    chunk_duration_sec: int,
    uvr_path: str,
    model: str = "",
    log_callback=None,
) -> Path | None:
    """
    Lance l'extraction des voix (commande construite via resolve_uvr5_command).
    Retourne le chemin du fichier Vocals généré, ou None en cas d'échec.
    """
    if not input_path.exists():
        return None
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    cmd = build_uvr5_command(uvr_path, input_path, output_dir, chunk_duration_sec, model)
    if not cmd:
        return None
    try:
        if log_callback:
            log_callback(f"UVR5: {' '.join(cmd)}")
        result = subprocess.run(
            cmd,
            capture_output=True,
            timeout=3600,
            cwd=str(output_dir),
        )
        if result.returncode != 0 and log_callback:
            err = (result.stderr or result.stdout or b"").decode(errors="replace").strip()
            if err:
                log_callback(f"UVR5 stderr: {err}")

        stem = input_path.stem
        for f in output_dir.iterdir():
            if f.suffix.lower() in (".wav", ".flac", ".mp3", ".ogg") and (
                "vocals" in f.name.lower() or "Vocals" in f.name
            ):
                return f
            if f.stem == stem and "vocals" in f.name.lower():
                return f
        for f in output_dir.iterdir():
            if stem in f.name and "vocal" in f.name.lower():
                return f
        return None
    except subprocess.TimeoutExpired:
        if log_callback:
            log_callback("UVR5: timeout.")
        return None
    except Exception as e:
        if log_callback:
            log_callback(f"UVR5: {type(e).__name__}: {e}")
        return None


def _parse_model_path(model: str) -> tuple[str, str]:
    """Retourne (model_file_dir, model_filename). Si model est un chemin, dossier + nom ; sinon ('', model)."""
    if not model:
        return "", ""
    path = Path(model)
    if "/" in model or "\\" in model:
        parent = path.parent
        if parent and str(parent) != ".":
            return str(parent.resolve()), path.name
    return "", path.name if path.name else model


def ensure_audio_separator_imported_in_main_thread(log_callback=None) -> bool:
    """
    Importe onnxruntime et audio_separator dans le thread principal (sous Windows,
    le chargement des DLL peut échouer dans un thread secondaire). À appeler avant
    de lancer le worker in-process. Retourne True si l'import a réussi.
    """
    if sys.platform == "win32":
        import site
        for sp in site.getsitepackages():
            if os.path.isdir(sp):
                try:
                    os.add_dll_directory(sp)
                except OSError:
                    pass
            capi = os.path.join(sp, "onnxruntime", "capi")
            if os.path.isdir(capi):
                try:
                    os.add_dll_directory(capi)
                except OSError:
                    pass
                break
    try:
        import onnxruntime  # noqa: F401
        from audio_separator.separator import Separator  # noqa: F401
        if log_callback:
            providers = onnxruntime.get_available_providers()
            log_callback(f"UVR5: ONNX Runtime providers disponibles: {', '.join(providers)}")
        return True
    except ImportError as e:
        if log_callback:
            log_callback(f"UVR5: ImportError lors de l'import: {e}")
        return False
    except Exception as e:
        if log_callback:
            log_callback(f"UVR5: Erreur inattendue lors de l'import: {type(e).__name__}: {e}")
        return False


def run_uvr5_vocals_inprocess(
    input_path: Path,
    output_dir: Path,
    chunk_duration_sec: int,
    model: str = "",
    mdx_segment_size: int | None = None,
    mdxc_overlap: int | None = None,
    mdx_batch_size: int | None = None,
    log_callback=None,
) -> Path | None:
    """
    Lance l'extraction Vocals via l'API Python de audio_separator avec use_directml=True (GPU AMD/Intel).
    À utiliser quand audio-separator est dans le même environnement (PATH / même Python).
    Retourne le chemin du fichier Vocals généré, ou None en cas d'échec.
    """
    if not input_path.exists():
        return None
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    if log_callback:
        log_callback(f"UVR5 in-process: Python utilisé = {sys.executable}")
    # Sous Windows, aider le chargement des DLL d'ONNX Runtime (éviter "DLL load failed")
    if sys.platform == "win32":
        import site
        for sp in site.getsitepackages():
            if os.path.isdir(sp):
                try:
                    os.add_dll_directory(sp)
                except OSError:
                    pass
            capi = os.path.join(sp, "onnxruntime", "capi")
            if os.path.isdir(capi):
                try:
                    os.add_dll_directory(capi)
                except OSError:
                    pass
                break
    
    try:
        # audio_separator est déjà importé dans app.py, donc cet import est rapide
        from audio_separator.separator import Separator
    except ImportError as e:
        if log_callback:
            err = str(e).strip() or repr(e)
            log_callback(f"UVR5 in-process: import audio_separator échoué: {err}")
            if "DLL load failed" in err or ("onnxruntime" in err.lower() and "numpy" not in err.lower()):
                log_callback(
                    "UVR5 in-process: installez le Visual C++ Redistributable (vc_redist.x64.exe) "
                    "puis dans le venv: pip uninstall onnxruntime onnxruntime-directml -y && pip install onnxruntime-directml"
                )
            if "numpy" in err.lower() and "1.x" in err.lower():
                log_callback(
                    "UVR5 in-process: onnxruntime-directml 1.17 exige NumPy 1.x. "
                    "Dans le venv: pip install \"numpy<2\" puis relancer."
                )
        return None

    model_dir, model_filename = _parse_model_path(model)
    if not model_filename and model:
        model_filename = model
    model_file_dir = model_dir if model_dir else "/tmp/audio-separator-models/"

    # Rediriger les logs audio_separator vers log_callback avec throttling pour éviter de bloquer l'UI
    class CallbackHandler(logging.Handler):
        def __init__(self, cb):
            super().__init__()
            self._cb = cb
            self._last_emit_time = 0
            self._last_progress_pct = -1
            self._throttle_seconds = 2.0  # N'émettre qu'un signal toutes les 2 secondes max

        def emit(self, record):
            if not self._cb:
                return
            msg = self.format(record)
            current_time = time.time()
            
            # Messages critiques toujours affichés immédiatement
            critical_keywords = [
                "Load model", "DirectML", "DmlExecutionProvider", "acceleration",
                "Separation complete", "complete!", "Error", "Failed", "WARNING", "Separator version"
            ]
            is_critical = any(keyword in msg for keyword in critical_keywords)
            
            # Progression des chunks avec throttling
            if "Processing chunk" in msg:
                try:
                    parts = msg.split("chunk")[1].strip().split("/")
                    current = int(parts[0].strip())
                    total = int(parts[1].split(":")[0].strip())
                    pct = int(100 * current / total)
                    
                    # Afficher uniquement si :
                    # 1. Le pourcentage a changé de plus de 5%
                    # 2. Ou si plus de 2 secondes se sont écoulées
                    if (pct - self._last_progress_pct >= 5) or (current_time - self._last_emit_time >= self._throttle_seconds):
                        self._last_progress_pct = pct
                        self._last_emit_time = current_time
                        self._cb(f"UVR5: Chunk {current}/{total} ({pct}%)")
                except (ValueError, IndexError):
                    pass
                return
            
            # Messages critiques affichés immédiatement
            if is_critical:
                self._last_emit_time = current_time
                self._cb(msg)
            # Autres messages avec throttling
            elif current_time - self._last_emit_time >= self._throttle_seconds:
                self._last_emit_time = current_time
                self._cb(msg)

    log_handler = CallbackHandler(log_callback)
    log_handler.setFormatter(logging.Formatter("%(levelname)s - %(module)s - %(message)s"))
    as_logger = logging.getLogger("audio_separator")
    as_logger.addHandler(log_handler)
    as_logger.setLevel(logging.INFO)
    
    try:
        sep = Separator(
            log_level=logging.INFO,
            model_file_dir=model_file_dir,
            output_dir=str(output_dir),
            output_format="FLAC",
            output_single_stem="Vocals",
            use_directml=True,
            chunk_duration=float(chunk_duration_sec),
            mdx_params={
                "hop_length": 1024,
                "segment_size": mdx_segment_size or UVR5_MDX_SEGMENT_SIZE_DEFAULT,
                "overlap": 0.25,
                "batch_size": mdx_batch_size or UVR5_MDX_BATCH_SIZE_DEFAULT,
                "enable_denoise": False,
            },
            mdxc_params={
                "segment_size": mdx_segment_size or UVR5_MDX_SEGMENT_SIZE_DEFAULT,
                "override_model_segment_size": False,
                "batch_size": mdx_batch_size or UVR5_MDX_BATCH_SIZE_DEFAULT,
                "overlap": mdxc_overlap or UVR5_MDXC_OVERLAP_DEFAULT,
                "pitch_shift": 0,
            },
        )
        if log_callback:
            log_callback("UVR5: Chargement du modèle...")
        sep.load_model(model_filename=model_filename or "model_bs_roformer_ep_317_sdr_12.9755.ckpt")
        if log_callback:
            log_callback("UVR5: Séparation en cours (DirectML activé)...")
        sep.separate([str(input_path)])
    except Exception as e:
        if log_callback:
            log_callback(f"UVR5 in-process: {type(e).__name__}: {e}")
        return None
    finally:
        as_logger.removeHandler(log_handler)

    stem = input_path.stem
    for f in output_dir.iterdir():
        if f.suffix.lower() in (".wav", ".flac", ".mp3", ".ogg") and (
            "vocals" in f.name.lower() or "Vocals" in f.name
        ):
            return f
        if f.stem == stem and "vocals" in f.name.lower():
            return f
    for f in output_dir.iterdir():
        if stem in f.name and "vocal" in f.name.lower():
            return f
    return None
