//! Conversion / transcription pipeline orchestration.

use crate::config::{data_dir, AppConfig};
use crate::queue::{CleanMode, ItemStatus, QueueItem};
use crossbeam_channel::{unbounded, Receiver, Sender};
use cs_dpdf::apply_dpdf;
use cs_ffmpeg::{
    build_convert_command, ffmpeg_available, probe_duration_sec, resolve_ffmpeg,
    run_command_with_progress,
};
use cs_inworld::apply_inworld;
use cs_uvr5::separate_vocals;
use cs_warmth::apply_warmth;
use cs_whisper::transcribe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("{0}")]
    Msg(String),
}

#[derive(Debug, Clone)]
pub enum PipelineEvent {
    Log(String),
    ItemProgress { index: usize, progress: u8 },
    ItemStatus { index: usize, status: ItemStatus, error: Option<String> },
    Finished { cancelled: bool },
}

pub struct PipelineHandle {
    pub cancel: Arc<AtomicBool>,
    pub rx: Receiver<PipelineEvent>,
}

pub fn spawn_convert(
    items: Vec<(usize, QueueItem)>,
    config: AppConfig,
    process_all_transcribe: bool,
) -> PipelineHandle {
    let (tx, rx) = unbounded();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_c = cancel.clone();
    std::thread::spawn(move || {
        run_pipeline(items, config, process_all_transcribe, false, cancel_c, tx);
    });
    PipelineHandle { cancel, rx }
}

pub fn spawn_transcribe_only(items: Vec<(usize, QueueItem)>, config: AppConfig) -> PipelineHandle {
    let (tx, rx) = unbounded();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_c = cancel.clone();
    std::thread::spawn(move || {
        run_pipeline(items, config, false, true, cancel_c, tx);
    });
    PipelineHandle { cancel, rx }
}

fn run_pipeline(
    items: Vec<(usize, QueueItem)>,
    config: AppConfig,
    process_all_transcribe: bool,
    transcribe_only: bool,
    cancel: Arc<AtomicBool>,
    tx: Sender<PipelineEvent>,
) {
    let data = data_dir();
    let log = |tx: &Sender<PipelineEvent>, s: String| {
        let _ = tx.send(PipelineEvent::Log(s));
    };

    if !transcribe_only && !ffmpeg_available(Some(&data)) {
        log(&tx, "FFmpeg introuvable — installez FFmpeg ou placez-le dans le dossier data.".into());
        let _ = tx.send(PipelineEvent::Finished { cancelled: false });
        return;
    }

    for (index, item) in &items {
        if cancel.load(Ordering::SeqCst) {
            let _ = tx.send(PipelineEvent::ItemStatus {
                index: *index,
                status: ItemStatus::Cancelled,
                error: None,
            });
            let _ = tx.send(PipelineEvent::Finished { cancelled: true });
            return;
        }

        let _ = tx.send(PipelineEvent::ItemStatus {
            index: *index,
            status: ItemStatus::Running,
            error: None,
        });

        let result = if transcribe_only {
            do_transcribe(*index, item, &config, &data, &tx, &cancel)
        } else {
            do_convert(*index, item, &config, &data, &tx, &cancel).and_then(|out| {
                if process_all_transcribe && item.transcribe {
                    do_transcribe_path(*index, &out, item, &config, &data, &tx)
                } else {
                    Ok(())
                }
            })
        };

        match result {
            Ok(()) => {
                let _ = tx.send(PipelineEvent::ItemStatus {
                    index: *index,
                    status: ItemStatus::Done,
                    error: None,
                });
                let _ = tx.send(PipelineEvent::ItemProgress {
                    index: *index,
                    progress: 100,
                });
            }
            Err(e) => {
                log(&tx, format!("Erreur: {e}"));
                let _ = tx.send(PipelineEvent::ItemStatus {
                    index: *index,
                    status: ItemStatus::Failed,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    let _ = tx.send(PipelineEvent::Finished {
        cancelled: cancel.load(Ordering::SeqCst),
    });
}

fn output_opus_path(item: &QueueItem, config: &AppConfig) -> PathBuf {
    let stem = item
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".into());
    let name = format!("{}{}.opus", stem, config.clean_suffix);
    if !config.output_dir.is_empty() {
        PathBuf::from(&config.output_dir).join(name)
    } else {
        item.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(name)
    }
}

fn do_convert(
    index: usize,
    item: &QueueItem,
    config: &AppConfig,
    data: &Path,
    tx: &Sender<PipelineEvent>,
    cancel: &AtomicBool,
) -> Result<PathBuf, PipelineError> {
    let _ = cancel;
    let ffmpeg = resolve_ffmpeg(Some(data)).map_err(|e| PipelineError::Msg(e.to_string()))?;
    let mut work = item.path.clone();
    let tmp = tempfile::tempdir().map_err(|e| PipelineError::Msg(e.to_string()))?;

    if item.isolate {
        let _ = tx.send(PipelineEvent::Log(format!(
            "UVR MDX: {}",
            item.display_name()
        )));
        let stem = separate_vocals(&work, tmp.path(), &config.uvr5, Some(data))
            .map_err(|e| PipelineError::Msg(e.to_string()))?;
        if config.keep_flac {
            let dest = output_opus_path(item, config).with_extension("flac");
            let _ = std::fs::copy(&stem, &dest);
            let _ = tx.send(PipelineEvent::Log(format!("FLAC conservé: {}", dest.display())));
        }
        work = stem;
    } else if item.clean == CleanMode::Dpdf {
        let _ = tx.send(PipelineEvent::Log(format!("DPDF: {}", item.display_name())));
        let out = tmp.path().join("dpdf.wav");
        apply_dpdf(&work, &out, &config.dpdf, Some(data))
            .map_err(|e| PipelineError::Msg(e.to_string()))?;
        work = out;
    } else if item.clean == CleanMode::Inworld {
        let _ = tx.send(PipelineEvent::Log(format!(
            "Inworld: {}",
            item.display_name()
        )));
        let out = tmp.path().join("inworld.wav");
        apply_inworld(&work, &out, &config.inworld, Some(data))
            .map_err(|e| PipelineError::Msg(e.to_string()))?;
        work = out;
    }

    // Mixage après nettoyage / UVR (désourdage + chaleur) — ordre voulu
    if item.warmth {
        let _ = tx.send(PipelineEvent::Log(format!(
            "Mixage: {}",
            item.display_name()
        )));
        let out = tmp.path().join("mix.wav");
        apply_warmth(&work, &out, &config.warmth)
            .map_err(|e| PipelineError::Msg(e.to_string()))?;
        work = out;
    }

    let out_path = output_opus_path(item, config);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PipelineError::Msg(e.to_string()))?;
    }
    let duration = probe_duration_sec(&ffmpeg, &work).unwrap_or(0.0);
    let cmd = build_convert_command(&ffmpeg, &work, &out_path, item.gain_db, &config.conversion);
    let txc = tx.clone();
    run_command_with_progress(&cmd, duration, |p| {
        let _ = txc.send(PipelineEvent::ItemProgress {
            index,
            progress: p,
        });
    })
    .map_err(|e| PipelineError::Msg(e.to_string()))?;

    let _ = tx.send(PipelineEvent::Log(format!(
        "Opus: {}",
        out_path.display()
    )));
    Ok(out_path)
}

fn do_transcribe(
    index: usize,
    item: &QueueItem,
    config: &AppConfig,
    data: &Path,
    tx: &Sender<PipelineEvent>,
    _cancel: &AtomicBool,
) -> Result<(), PipelineError> {
    let audio = if item.path.extension().and_then(|e| e.to_str()) == Some("opus") {
        item.path.clone()
    } else {
        // Prefer converted opus next to source if exists
        let opus = output_opus_path(item, config);
        if opus.is_file() {
            opus
        } else {
            item.path.clone()
        }
    };
    do_transcribe_path(index, &audio, item, config, data, tx)
}

fn do_transcribe_path(
    index: usize,
    audio: &Path,
    item: &QueueItem,
    config: &AppConfig,
    data: &Path,
    tx: &Sender<PipelineEvent>,
) -> Result<(), PipelineError> {
    let _ = index;
    let _ = item;
    let base = audio.with_extension("");
    let _ = tx.send(PipelineEvent::Log(format!(
        "Transcription: {}",
        audio.display()
    )));
    let out = transcribe(audio, &base, &config.whisper, Some(data))
        .map_err(|e| PipelineError::Msg(e.to_string()))?;
    let _ = tx.send(PipelineEvent::Log(format!("Transcript: {}", out.display())));
    Ok(())
}
