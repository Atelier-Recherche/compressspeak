# FFmpeg embarqué

Placez les binaires FFmpeg ici (non versionnés) :

```
vendor/ffmpeg/windows/ffmpeg.exe
vendor/ffmpeg/macos/ffmpeg
vendor/ffmpeg/linux/ffmpeg
```

Au runtime, Compress Speak cherche aussi :
1. `%LOCALAPPDATA%/Compress Speak/ffmpeg/...` (ou équivalent macOS/Linux via `directories`)
2. `ffmpeg` dans le `PATH`

Téléchargements suggérés :
- Windows : https://www.gyan.dev/ffmpeg/builds/ (essentials)
- macOS : `brew install ffmpeg` ou binaire statique
- Linux : `ffmpeg` du dépôt, ou build static John Van Sickle
