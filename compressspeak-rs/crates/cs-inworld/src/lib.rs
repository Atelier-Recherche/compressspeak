//! Inworld Voice Isolator — moteur ONNX natif (pas de Python).
//! Modèle : HuggingFace `mrfakename/voice-isolator-onnx` (conversion Apache-2.0
//! du checkpoint offline `inworld/voice-isolator`).

mod engine;
mod stft;

use cs_ffmpeg::{build_mono_wav_command, resolve_ffmpeg, run_command};
use engine::InworldSession;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;

pub use stft::{CHUNK, HOP, N_FFT, OVERLAP, SR};

#[derive(Debug, Error)]
pub enum InworldError {
    #[error("FFmpeg: {0}")]
    Ffmpeg(String),
    #[error("Inworld: {0}")]
    Engine(String),
    #[error("téléchargement modèle: {0}")]
    Download(String),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InworldParams {
    /// Intensité du nettoyage : 0 = original, 1 = isolation complète.
    /// Défaut 0.75 pour moins « manger » la voix qu’à 100 %.
    #[serde(default = "default_strength")]
    pub strength: f32,
    /// Plafond d’atténuation (dB) par rapport à l’original.
    /// 0 = désactivé ; 6–12 évite que le modèle creuse trop les passages de voix.
    #[serde(default = "default_voice_protect_db")]
    pub voice_protect_db: f32,
}

fn default_strength() -> f32 {
    0.75
}

fn default_voice_protect_db() -> f32 {
    6.0
}

impl Default for InworldParams {
    fn default() -> Self {
        Self {
            strength: default_strength(),
            voice_protect_db: default_voice_protect_db(),
        }
    }
}

impl InworldParams {
    pub fn normalize(&mut self) {
        if !self.strength.is_finite() {
            self.strength = default_strength();
        }
        self.strength = self.strength.clamp(0.0, 1.0);
        if !self.voice_protect_db.is_finite() {
            self.voice_protect_db = default_voice_protect_db();
        }
        self.voice_protect_db = self.voice_protect_db.clamp(0.0, 24.0);
    }
}

pub const MODEL_FILE: &str = "voice_isolator_offline.onnx";
pub const MODEL_ID: &str = "voice_isolator_offline";

fn hf_url() -> &'static str {
    "https://huggingface.co/mrfakename/voice-isolator-onnx/resolve/main/voice_isolator_offline.onnx"
}

pub fn models_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "CompressSpeak", "Compress Speak")
        .map(|d| d.data_local_dir().join("models").join("inworld"))
        .unwrap_or_else(|| PathBuf::from("models/inworld"))
}

pub fn find_local_model() -> Option<PathBuf> {
    let p = models_dir().join(MODEL_FILE);
    if p.is_file() && p.metadata().map(|m| m.len() > 10_000_000).unwrap_or(false) {
        Some(p)
    } else {
        None
    }
}

pub fn model_is_ready() -> bool {
    find_local_model().is_some()
}

pub fn is_available() -> bool {
    true
}

/// Télécharge le modèle ONNX (~249 Mo) si absent.
pub fn ensure_model() -> Result<PathBuf, InworldError> {
    if let Some(p) = find_local_model() {
        return Ok(p);
    }
    let dest_dir = models_dir();
    std::fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(MODEL_FILE);
    if dest.is_file() && dest.metadata()?.len() > 10_000_000 {
        return Ok(dest);
    }

    tracing::info!("Téléchargement Inworld Voice Isolator ONNX…");
    let resp = ureq::get(hf_url())
        .call()
        .map_err(|e| InworldError::Download(e.to_string()))?;
    let mut body = resp.into_body().into_reader();
    let tmp = dest.with_extension("onnx.part");
    {
        let mut file = std::fs::File::create(&tmp)?;
        let mut buf = [0u8; 1024 * 256];
        let mut total = 0u64;
        loop {
            let n = std::io::Read::read(&mut body, &mut buf)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            total += n as u64;
        }
        file.flush()?;
        if total < 10_000_000 {
            let _ = std::fs::remove_file(&tmp);
            return Err(InworldError::Download(format!(
                "fichier trop petit ({total} octets)"
            )));
        }
    }
    std::fs::rename(&tmp, &dest)?;
    Ok(dest)
}

static SESSION: OnceLock<Mutex<Option<InworldSession>>> = OnceLock::new();

fn session_slot() -> &'static Mutex<Option<InworldSession>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

fn with_session<R>(f: impl FnOnce(&mut InworldSession) -> Result<R, InworldError>) -> Result<R, InworldError> {
    let path = ensure_model()?;
    let mut slot = session_slot().lock();
    if slot.is_none() {
        let s = InworldSession::load(&path).map_err(InworldError::Engine)?;
        *slot = Some(s);
    }
    let session = slot
        .as_mut()
        .ok_or_else(|| InworldError::Engine("session absente".into()))?;
    f(session)
}

/// Mélange dry/wet + plafond d’atténuation pour préserver la voix.
pub fn blend_voice(dry: &[f32], wet: &[f32], params: &InworldParams) -> Vec<f32> {
    let mut p = params.clone();
    p.normalize();
    let strength = p.strength;
    let floor = if p.voice_protect_db > 0.0 {
        10f32.powf(-p.voice_protect_db / 20.0)
    } else {
        0.0
    };
    let n = dry.len().min(wet.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let d = dry[i];
        let w = wet[i];
        // out = dry + strength * (wet - dry)
        let mut y = d + strength * (w - d);
        if floor > 0.0 {
            let a = d.abs();
            if a > 1e-6 {
                let min_amp = a * floor;
                if y.abs() < min_amp {
                    // Garde au moins le plancher d’amplitude de l’original (signe du wet).
                    let sign = if y != 0.0 {
                        y.signum()
                    } else if w != 0.0 {
                        w.signum()
                    } else {
                        d.signum()
                    };
                    y = sign * min_amp;
                }
            }
        }
        out.push(y);
    }
    out
}

pub fn apply_inworld(
    input: &Path,
    output: &Path,
    params: &InworldParams,
    data_dir: Option<&Path>,
) -> Result<(), InworldError> {
    let mut params = params.clone();
    params.normalize();

    let ffmpeg = resolve_ffmpeg(data_dir).map_err(|e| InworldError::Ffmpeg(e.to_string()))?;
    let tmp = tempfile::tempdir()?;
    let wav_in = tmp.path().join("input_48k.wav");
    let cmd = build_mono_wav_command(&ffmpeg, input, &wav_in, SR);
    run_command(&cmd).map_err(|e| InworldError::Ffmpeg(e.to_string()))?;

    let (audio, file_sr) = read_wav_mono_f32(&wav_in)?;
    if file_sr != SR {
        return Err(InworldError::Engine(format!(
            "sample rate {file_sr} != {SR} après resampling FFmpeg"
        )));
    }

    let wet = with_session(|session| {
        session
            .isolate(&audio)
            .map_err(InworldError::Engine)
    })?;
    let voice = blend_voice(&audio, &wet, &params);

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_wav_pcm16(output, &voice, SR)?;
    tracing::info!(
        "Inworld Voice Isolator OK (strength={:.0}%, protect={:.0} dB) → {}",
        params.strength * 100.0,
        params.voice_protect_db,
        output.display()
    );
    Ok(())
}

fn read_wav_mono_f32(path: &Path) -> Result<(Vec<f32>, u32), InworldError> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| InworldError::Io(std::io::Error::other(e)))?;
    let spec = reader.spec();
    let sr = spec.sample_rate;
    let ch = spec.channels as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.unwrap_or(0) as f32 / max)
                .collect()
        }
    };
    if ch <= 1 {
        return Ok((samples, sr));
    }
    let frames = samples.len() / ch;
    let mut mono = Vec::with_capacity(frames);
    for i in 0..frames {
        let mut sum = 0.0;
        for c in 0..ch {
            sum += samples[i * ch + c];
        }
        mono.push(sum / ch as f32);
    }
    Ok((mono, sr))
}

fn write_wav_pcm16(path: &Path, samples: &[f32], sr: u32) -> Result<(), InworldError> {
    let mut peak = 0f32;
    for &s in samples {
        peak = peak.max(s.abs());
    }
    let scale = if peak > 1.0 { 1.0 / peak } else { 1.0 };
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| InworldError::Io(std::io::Error::other(e)))?;
    for &s in samples {
        let v = (s * scale).clamp(-1.0, 1.0);
        writer
            .write_sample((v * 32767.0).round() as i16)
            .map_err(|e| InworldError::Io(std::io::Error::other(e)))?;
    }
    writer
        .finalize()
        .map_err(|e| InworldError::Io(std::io::Error::other(e)))?;
    Ok(())
}
