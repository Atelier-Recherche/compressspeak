#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""
Script worker pour UVR5 avec DirectML.
Lance audio-separator en processus séparé avec DirectML activé.
"""

import sys
import os
import logging
from pathlib import Path

# Désactiver tqdm pour éviter les milliers de lignes de progression
os.environ["TQDM_DISABLE"] = "1"

# Configurer les chemins DLL sous Windows
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

def main():
    if len(sys.argv) < 3:
        print("Usage: uvr5_worker.py <input_file> <output_dir> [chunk_duration] [model_file_dir] [model_filename] [mdx_segment_size] [mdxc_overlap] [mdx_batch_size] [directml_device] [stem]", file=sys.stderr)
        sys.exit(1)
    
    input_path = Path(sys.argv[1])
    output_dir = Path(sys.argv[2])
    chunk_duration = float(sys.argv[3]) if len(sys.argv) > 3 else 1200.0
    model_file_dir = sys.argv[4] if len(sys.argv) > 4 else "/tmp/audio-separator-models/"
    model_filename = sys.argv[5] if len(sys.argv) > 5 else "model_bs_roformer_ep_317_sdr_12.9755.ckpt"
    mdx_segment_size = int(sys.argv[6]) if len(sys.argv) > 6 else 256
    mdxc_overlap = int(sys.argv[7]) if len(sys.argv) > 7 else 8
    mdx_batch_size = int(sys.argv[8]) if len(sys.argv) > 8 else 1
    directml_device = int(sys.argv[9]) if len(sys.argv) > 9 else -1  # -1 = auto (défaut)
    stem = sys.argv[10] if len(sys.argv) > 10 else "Vocals"  # Stem à extraire
    
    output_dir.mkdir(parents=True, exist_ok=True)
    
    # Configurer le logging simplifié avec flush immédiat
    logging.basicConfig(
        level=logging.INFO,
        format='%(levelname)s - %(module)s - %(message)s',
        stream=sys.stdout,
        force=True
    )
    logger = logging.getLogger(__name__)
    
    # Forcer le flush de stdout pour éviter le buffering
    sys.stdout.reconfigure(line_buffering=True)
    
    # Importer et patcher audio_separator pour utiliser le bon adapteur DirectML
    try:
        # Patcher torch_directml AVANT que audio_separator l'importe
        if directml_device >= 0:
            import torch_directml
            device_count = torch_directml.device_count()
            logger.info(f">>> {device_count} adapteurs DirectML détectés")
            
            if directml_device < device_count:
                device_name = torch_directml.device_name(directml_device)
                logger.info(f">>> Configuration pour utiliser l'adapteur {directml_device}: {device_name}")
                sys.stdout.flush()
                
                # Patcher default_device() et device() pour forcer l'adapteur souhaité
                _original_default_device = torch_directml.default_device
                _original_device = torch_directml.device
                
                torch_directml.default_device = lambda: directml_device
                torch_directml.device = lambda device_id=None: _original_device(directml_device if device_id is None else device_id)
                
                # Patcher aussi dans torch_directml.dml si il existe
                if hasattr(torch_directml, 'dml'):
                    torch_directml.dml.default_device = lambda: directml_device
                    torch_directml.dml.device = lambda device_id=None: _original_device(directml_device if device_id is None else device_id)
                
                logger.info(f">>> Patch appliqué : default_device() retourne maintenant {directml_device}")
                sys.stdout.flush()
            else:
                logger.warning(f">>> Adapteur {directml_device} invalide (max={device_count-1})")
                sys.stdout.flush()
        
        from audio_separator.separator import Separator
        
        logger.info(f">>> Initialisation Separator avec DirectML...")
        sys.stdout.flush()
        
        logger.info(f">>> Stem à extraire: {stem}")
        sys.stdout.flush()
        
        sep = Separator(
            log_level=logging.INFO,
            model_file_dir=model_file_dir,
            output_dir=str(output_dir),
            output_format="FLAC",
            output_single_stem=stem,  # Utiliser le stem spécifié
            use_directml=True,  # ACTIVER DirectML !
            chunk_duration=chunk_duration,
            mdx_params={
                "hop_length": 1024,
                "segment_size": mdx_segment_size,
                "overlap": 0.25,
                "batch_size": mdx_batch_size,
                "enable_denoise": False,
            },
            mdxc_params={
                "segment_size": mdx_segment_size,
                "override_model_segment_size": False,
                "batch_size": mdx_batch_size,
                "overlap": mdxc_overlap,
                "pitch_shift": 0,
            },
        )
        
        logger.info(f">>> Chargement du modèle {model_filename}...")
        sys.stdout.flush()
        sep.load_model(model_filename=model_filename)
        
        logger.info(f">>> Séparation en cours avec DirectML...")
        logger.info(f">>> Fichier: {input_path.name} ({input_path.stat().st_size / (1024*1024):.1f} MB, chunk_size={chunk_duration}s)")
        sys.stdout.flush()
        
        output_files = sep.separate([str(input_path)])
        sys.stdout.flush()
        
        if output_files:
            output_file = output_files[0]
            # S'assurer que c'est un chemin absolu
            if not Path(output_file).is_absolute():
                output_file = str(output_dir / output_file)
            
            logger.info(f">>> Séparation terminée: {output_file}")
            sys.stdout.flush()
            
            # Vérifier que le fichier existe vraiment
            if Path(output_file).exists():
                print(f"STEM_OUTPUT:{output_file}")
                sys.stdout.flush()
            else:
                logger.error(f">>> Fichier de sortie introuvable: {output_file}")
                # Lister le contenu du dossier pour debug
                logger.error(f">>> Contenu de {output_dir}:")
                for f in output_dir.iterdir():
                    logger.error(f">>>   - {f.name}")
                sys.stdout.flush()
                sys.exit(1)
        else:
            logger.error(">>> Aucun fichier de sortie généré")
            sys.stdout.flush()
            sys.exit(1)
            
    except Exception as e:
        logger.error(f">>> Erreur: {type(e).__name__}: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)

if __name__ == "__main__":
    main()
