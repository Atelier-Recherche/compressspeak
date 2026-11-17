//! Lecteur audio avec position / durée / seek (rodio).

use rodio::Source;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct AudioPlayer {
    _stream: Option<rodio::OutputStream>,
    handle: Option<rodio::OutputStreamHandle>,
    sink: Option<rodio::Sink>,
    pub current: Option<PathBuf>,
    pub full_path: Option<PathBuf>,
    pub is_preview: bool,
    pub duration: Duration,
    pub return_position: Duration,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlayer {
    pub fn new() -> Self {
        match rodio::OutputStream::try_default() {
            Ok((stream, handle)) => Self {
                _stream: Some(stream),
                handle: Some(handle),
                sink: None,
                current: None,
                full_path: None,
                is_preview: false,
                duration: Duration::ZERO,
                return_position: Duration::ZERO,
            },
            Err(e) => {
                tracing::warn!("Audio output indisponible: {e}");
                Self {
                    _stream: None,
                    handle: None,
                    sink: None,
                    current: None,
                    full_path: None,
                    is_preview: false,
                    duration: Duration::ZERO,
                    return_position: Duration::ZERO,
                }
            }
        }
    }

    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        self.load_inner(path, false)
    }

    pub fn load_preview(&mut self, path: &Path, return_pos: Duration) -> Result<(), String> {
        self.return_position = return_pos;
        self.load_inner(path, true)
    }

    fn load_inner(&mut self, path: &Path, preview: bool) -> Result<(), String> {
        self.stop_sink_only();
        let handle = self
            .handle
            .as_ref()
            .ok_or_else(|| "sortie audio indisponible".to_string())?;
        let file = File::open(path).map_err(|e| e.to_string())?;
        let sink = rodio::Sink::try_new(handle).map_err(|e| e.to_string())?;
        let source = rodio::Decoder::new(BufReader::new(file)).map_err(|e| e.to_string())?;
        let mut duration = source.total_duration().unwrap_or(Duration::ZERO);
        if duration.is_zero() {
            // Fallback : ffprobe / ffmpeg pour les formats sans durée dans le décodeur
            if let Ok(ff) = cs_ffmpeg::resolve_ffmpeg(Some(&cs_core::data_dir())) {
                if let Some(secs) = cs_ffmpeg::probe_duration_sec(&ff, path) {
                    duration = Duration::from_secs_f64(secs);
                }
            }
        }
        sink.append(source);
        sink.pause();
        self.sink = Some(sink);
        self.current = Some(path.to_path_buf());
        if !preview {
            self.full_path = Some(path.to_path_buf());
            self.is_preview = false;
        } else {
            self.is_preview = true;
        }
        self.duration = duration;
        Ok(())
    }

    pub fn play(&mut self) {
        if let Some(s) = &self.sink {
            s.play();
        }
    }

    pub fn pause(&mut self) {
        if let Some(s) = &self.sink {
            s.pause();
        }
    }

    pub fn toggle(&mut self) {
        if self.is_paused() {
            self.play();
        } else {
            self.pause();
        }
    }

    pub fn stop(&mut self) {
        self.stop_sink_only();
        if let Some(path) = self.full_path.clone() {
            let _ = self.load(&path);
        } else {
            self.current = None;
            self.duration = Duration::ZERO;
            self.is_preview = false;
        }
    }

    fn stop_sink_only(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.sink.as_ref().map(|s| s.is_paused()).unwrap_or(true)
    }

    pub fn has_track(&self) -> bool {
        self.sink.is_some()
    }

    pub fn position(&self) -> Duration {
        self.sink
            .as_ref()
            .map(|s| s.get_pos())
            .unwrap_or(Duration::ZERO)
    }

    pub fn seek(&mut self, pos: Duration) {
        let Some(sink) = &self.sink else {
            return;
        };
        let _ = sink.try_seek(pos);
    }

    pub fn position_secs(&self) -> f32 {
        self.position().as_secs_f32()
    }

    pub fn duration_secs(&self) -> f32 {
        self.duration.as_secs_f32().max(0.001)
    }

    pub fn format_time(secs: f32) -> String {
        let total = secs.max(0.0) as u64;
        let h = total / 3600;
        let m = (total % 3600) / 60;
        let s = total % 60;
        if h > 0 {
            format!("{h:02}:{m:02}:{s:02}")
        } else {
            format!("{m:02}:{s:02}")
        }
    }
}
