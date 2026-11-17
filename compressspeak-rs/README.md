# CompressSpeak (Rust)

Documentation utilisateur : [README.md](../README.md) (aussi [en](../README.en.md) · [de](../README.de.md) · [es](../README.es.md)).

## Dev

```bash
cargo run -p compressspeak
cargo build --release -p compressspeak
```

FFmpeg embarqué : `scripts/fetch-ffmpeg.ps1` (Windows) ou `scripts/fetch-ffmpeg.sh` (macOS / Linux).

Workspace : `crates/compressspeak`, `cs-core`, `cs-ffmpeg`, `cs-warmth`, `cs-dpdf`, `cs-whisper`, `cs-uvr5`.
