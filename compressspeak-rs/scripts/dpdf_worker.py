#!/usr/bin/env python3
"""Worker DPDFNet (même logique que HushMic Gradio / dpdfnet).

Usage:
  python dpdf_worker.py <input> <output_wav> <model_id> [attn_limit_db]
"""
from __future__ import annotations

import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) < 4:
        print(
            "usage: dpdf_worker.py <input> <output_wav> <model_id> [attn_db]",
            file=sys.stderr,
        )
        return 2
    inp = Path(sys.argv[1])
    out = Path(sys.argv[2])
    model = sys.argv[3].strip()
    attn_raw = float(sys.argv[4]) if len(sys.argv) > 4 else 0.0
    attn = attn_raw if attn_raw > 0 else None

    try:
        import numpy as np
        import soundfile as sf
        from dpdfnet.api import _enhance_with_runtime
        from dpdfnet.audio import pcm16_safe
        from dpdfnet.models import resolve_model
        from dpdfnet.onnx_backend import build_runtime_model
    except ImportError as e:
        print(f"dpdfnet non installé: {e}", file=sys.stderr)
        return 3

    if not inp.is_file():
        print(f"fichier introuvable: {inp}", file=sys.stderr)
        return 4

    resolved = resolve_model(model=model, onnx_path=None, auto_download=True, verbose=False)
    runtime = build_runtime_model(resolved.onnx_path)
    model_sr = int(resolved.info.sample_rate)

    audio, file_sr = sf.read(str(inp), always_2d=False)
    audio = np.asarray(audio, dtype=np.float32)
    enhanced = _enhance_with_runtime(
        audio,
        int(file_sr),
        runtime=runtime,
        model_sample_rate=model_sr,
        attn_limit_db=attn,
    )
    out.parent.mkdir(parents=True, exist_ok=True)
    sf.write(str(out), pcm16_safe(enhanced), int(file_sr), subtype="PCM_16")
    print(f"OK {model} → {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
