# -*- coding: utf-8 -*-
"""Module pour ajouter de la chaleur vocale avec Pedalboard."""

from __future__ import annotations
from pathlib import Path
import logging

try:
    from pedalboard import Pedalboard, HighShelfFilter, Distortion, Compressor, Gain
    from pedalboard.io import AudioFile
    PEDALBOARD_AVAILABLE = True
except ImportError:
    PEDALBOARD_AVAILABLE = False

logger = logging.getLogger(__name__)


def apply_warmth(
    input_path: Path,
    output_path: Path,
    high_shelf_gain_db: float = 2.0,
    high_shelf_cutoff_hz: float = 400.0,
    distortion_drive_db: float = 3.0,
    compressor_threshold_db: float = -15.0,
    compressor_ratio: float = 3.0,
    final_gain_db: float = 2.0,
) -> bool:
    """
    Applique des effets de "chaleur" vocale à un fichier audio.
    
    Args:
        input_path: Fichier audio source (FLAC/WAV)
        output_path: Fichier audio de sortie
        high_shelf_gain_db: Gain du filtre passe-haut (corps)
        high_shelf_cutoff_hz: Fréquence de coupure du filtre
        distortion_drive_db: Intensité de la distortion (harmoniques)
        compressor_threshold_db: Seuil du compresseur
        compressor_ratio: Ratio de compression
        final_gain_db: Gain final pour compenser
    
    Returns:
        True si succès, False sinon
    """
    if not PEDALBOARD_AVAILABLE:
        logger.error("Pedalboard n'est pas installé")
        return False
    
    if not input_path.exists():
        logger.error(f"Fichier source introuvable: {input_path}")
        return False
    
    try:
        logger.info(f"Application de la chaleur vocale: {input_path.name}")
        
        # Charger l'audio
        with AudioFile(str(input_path)) as f:
            audio = f.read(f.frames)
            samplerate = f.samplerate
            num_channels = audio.shape[0]
        
        # Créer la chaîne d'effets
        board = Pedalboard([
            # Redonner du corps dans les médiums-graves
            HighShelfFilter(
                cutoff_frequency_hz=high_shelf_cutoff_hz,
                gain_db=high_shelf_gain_db,
                q=1.0
            ),
            # Ajouter des harmoniques (le "chaud")
            Distortion(drive_db=distortion_drive_db),
            # Lisser le volume
            Compressor(
                threshold_db=compressor_threshold_db,
                ratio=compressor_ratio
            ),
            # Gain final pour compenser
            Gain(gain_db=final_gain_db)
        ])
        
        # Appliquer les effets
        logger.info(f"  - HighShelf: {high_shelf_cutoff_hz}Hz, +{high_shelf_gain_db}dB")
        logger.info(f"  - Distortion: {distortion_drive_db}dB drive")
        logger.info(f"  - Compressor: {compressor_threshold_db}dB @ {compressor_ratio}:1")
        logger.info(f"  - Gain final: +{final_gain_db}dB")
        
        effected = board(audio, samplerate)
        
        # Sauvegarder
        with AudioFile(str(output_path), 'w', samplerate, num_channels) as f:
            f.write(effected)
        
        logger.info(f"Chaleur vocale appliquée: {output_path}")
        return True
        
    except Exception as e:
        logger.error(f"Erreur lors de l'application de la chaleur: {e}")
        import traceback
        traceback.print_exc()
        return False


def is_pedalboard_available() -> bool:
    """Vérifie si Pedalboard est disponible."""
    return PEDALBOARD_AVAILABLE
