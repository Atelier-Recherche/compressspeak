//! DPDFNet (Ceva) — moteur ONNX natif intégré (pas de Python).
//! Modèles téléchargés depuis HuggingFace `Ceva-IP/DPDFNet`.

mod engine;
mod stft;

use cs_ffmpeg::{build_mono_wav_command, resolve_ffmpeg, run_command};
use engine::DpdfSession;
#[cfg(test)]
use engine::enhance_mono;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DpdfError {
    #[error("FFmpeg: {0}")]
    Ffmpeg(String),
    #[error("DPDFNet: {0}")]
    Engine(String),
    #[error("téléchargement modèle: {0}")]
    Download(String),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
}

pub const SAMPLE_RATES: &[&str] = &["8 kHz", "16 kHz", "48 kHz"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DpdfParams {
    pub sample_rate: String,
    pub model_label: String,
    #[serde(default)]
    pub model: String,
    pub attn_limit_db: f32,
}

impl Default for DpdfParams {
    fn default() -> Self {
        Self {
            sample_rate: "16 kHz".into(),
            model_label: "DPDFNet2".into(),
            model: "dpdfnet2".into(),
            attn_limit_db: 0.0,
        }
    }
}

impl DpdfParams {
    pub fn normalize(&mut self) {
        if !SAMPLE_RATES.contains(&self.sample_rate.as_str()) {
            if self.model.contains("48khz") {
                self.sample_rate = "48 kHz".into();
            } else if self.model.contains("8khz") {
                self.sample_rate = "8 kHz".into();
            } else {
                self.sample_rate = "16 kHz".into();
            }
        }
        let labels = model_labels_for_sr(&self.sample_rate);
        if !labels.iter().any(|l| *l == self.model_label) {
            self.model_label = match self.model.as_str() {
                "baseline" => "Baseline".into(),
                "dpdfnet4" => "DPDFNet4".into(),
                "dpdfnet8" | "dpdfnet8_8khz" | "dpdfnet8_48khz_hr" => "DPDFNet8".into(),
                _ => "DPDFNet2".into(),
            };
            if !labels.iter().any(|l| *l == self.model_label) {
                self.model_label = labels
                    .iter()
                    .find(|l| **l == "DPDFNet2")
                    .or(labels.first())
                    .copied()
                    .unwrap_or("DPDFNet2")
                    .to_string();
            }
        }
        self.model = resolve_model_id(&self.sample_rate, &self.model_label).to_string();
    }

    pub fn model_id(&self) -> String {
        let mut p = self.clone();
        p.normalize();
        p.model
    }

    pub fn sample_rate_hz(&self) -> u32 {
        match self.sample_rate.as_str() {
            "8 kHz" => 8000,
            "48 kHz" => 48000,
            _ => 16000,
        }
    }
}

pub fn models_for_sr(sr_label: &str) -> &'static [(&'static str, &'static str)] {
    match sr_label {
        "8 kHz" => &[("DPDFNet2", "dpdfnet2_8khz"), ("DPDFNet8", "dpdfnet8_8khz")],
        "48 kHz" => &[
            ("DPDFNet2", "dpdfnet2_48khz_hr"),
            ("DPDFNet8", "dpdfnet8_48khz_hr"),
        ],
        _ => &[
            ("Baseline", "baseline"),
            ("DPDFNet2", "dpdfnet2"),
            ("DPDFNet4", "dpdfnet4"),
            ("DPDFNet8", "dpdfnet8"),
        ],
    }
}

pub fn model_labels_for_sr(sr_label: &str) -> Vec<&'static str> {
    models_for_sr(sr_label).iter().map(|(l, _)| *l).collect()
}

pub fn resolve_model_id(sr_label: &str, model_label: &str) -> &'static str {
    for (label, id) in models_for_sr(sr_label) {
        if *label == model_label {
            return id;
        }
    }
    models_for_sr(sr_label)
        .iter()
        .find(|(l, _)| *l == "DPDFNet2")
        .map(|(_, id)| *id)
        .unwrap_or("dpdfnet2")
}

pub const AVAILABLE_MODELS: &[&str] = &[
    "baseline",
    "dpdfnet2",
    "dpdfnet4",
    "dpdfnet8",
    "dpdfnet2_8khz",
    "dpdfnet8_8khz",
    "dpdfnet2_48khz_hr",
    "dpdfnet8_48khz_hr",
];

fn onnx_filename(model_id: &str) -> String {
    format!("{model_id}.onnx")
}

fn hf_url(model_id: &str) -> String {
    format!(
        "https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/{}",
        onnx_filename(model_id)
    )
}

pub fn models_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "CompressSpeak", "Compress Speak")
        .map(|d| d.data_local_dir().join("models").join("dpdf"))
        .unwrap_or_else(|| PathBuf::from("models/dpdf"))
}

/// Also reuse Python dpdfnet cache if present.
fn candidate_model_paths(model_id: &str) -> Vec<PathBuf> {
    let name = onnx_filename(model_id);
    let mut v = vec![models_dir().join(&name)];
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        v.push(PathBuf::from(local).join("dpdfnet").join("models").join(&name));
    }
    if let Some(home) = directories::BaseDirs::new() {
        v.push(
            home.cache_dir()
                .join("dpdfnet")
                .join("models")
                .join(&name),
        );
    }
    v
}

/// Chemin du modèle s'il est déjà présent localement (sans réseau).
pub fn find_local_model(model_id: &str) -> Option<PathBuf> {
    for p in candidate_model_paths(model_id) {
        if p.is_file() && p.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false) {
            return Some(p);
        }
    }
    None
}

pub fn model_is_ready(model_id: &str) -> bool {
    find_local_model(model_id).is_some()
}

pub fn ensure_model(model_id: &str) -> Result<PathBuf, DpdfError> {
    if let Some(p) = find_local_model(model_id) {
        return Ok(p);
    }
    let dest_dir = models_dir();
    std::fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(onnx_filename(model_id));
    if dest.is_file() && dest.metadata()?.len() > 1_000_000 {
        return Ok(dest);
    }
    tracing::info!("Téléchargement DPDFNet {model_id} (Ceva-IP/DPDFNet)…");
    let url = hf_url(model_id);
    let resp = ureq::get(&url)
        .call()
        .map_err(|e| DpdfError::Download(e.to_string()))?;
    let bytes = resp
        .into_body()
        .read_to_vec()
        .map_err(|e| DpdfError::Download(e.to_string()))?;
    if bytes.len() < 1_000_000 {
        return Err(DpdfError::Download(format!(
            "fichier trop petit ({} octets) pour {model_id}",
            bytes.len()
        )));
    }
    let tmp = dest.with_extension("onnx.part");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(dest)
}

pub fn is_available() -> bool {
    true
}

/// True once ORT + at least one model can be prepared (network may still be needed).
pub fn dpdfnet_native_available() -> bool {
    true
}

/// Kept for UI: always true for native path (no Python required).
pub fn dpdfnet_python_available() -> bool {
    false
}

static SESSION_CACHE: OnceLock<Mutex<HashMap<String, DpdfSession>>> = OnceLock::new();

fn session_cache() -> &'static Mutex<HashMap<String, DpdfSession>> {
    SESSION_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_session(model_id: &str) -> Result<(), DpdfError> {
    let mut cache = session_cache().lock();
    if cache.contains_key(model_id) {
        return Ok(());
    }
    let path = ensure_model(model_id)?;
    let session = DpdfSession::load(&path).map_err(DpdfError::Engine)?;
    cache.insert(model_id.to_string(), session);
    Ok(())
}

pub fn apply_dpdf(
    input: &Path,
    output: &Path,
    params: &DpdfParams,
    data_dir: Option<&Path>,
) -> Result<(), DpdfError> {
    let mut params = params.clone();
    params.normalize();
    let model_id = params.model.clone();
    let sr = params.sample_rate_hz();

    let ffmpeg = resolve_ffmpeg(data_dir).map_err(|e| DpdfError::Ffmpeg(e.to_string()))?;
    let tmp = tempfile::tempdir()?;
    let wav_in = tmp.path().join(format!("input_{sr}.wav"));
    let cmd = build_mono_wav_command(&ffmpeg, input, &wav_in, sr);
    run_command(&cmd).map_err(|e| DpdfError::Ffmpeg(e.to_string()))?;

    get_session(&model_id)?;
    let (audio, file_sr) = read_wav_mono_f32(&wav_in)?;

    let enhanced = {
        let mut cache = session_cache().lock();
        let session = cache
            .get_mut(&model_id)
            .ok_or_else(|| DpdfError::Engine("session absente".into()))?;
        engine::enhance_mono(&audio, session, params.attn_limit_db).map_err(DpdfError::Engine)?
    };

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_wav_pcm16(output, &enhanced, file_sr)?;
    tracing::info!(
        "DPDFNet natif OK {} / {} → {}",
        params.sample_rate,
        params.model_label,
        output.display()
    );
    Ok(())
}

fn read_wav_mono_f32(path: &Path) -> Result<(Vec<f32>, u32), DpdfError> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| DpdfError::Io(std::io::Error::other(e)))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_baseline_and_enhances_silence() {
        let path = ensure_model("baseline").expect("download baseline");
        assert!(path.is_file());
        let mut session = DpdfSession::load(&path).expect("load onnx");
        assert_eq!(session.n_fft, 320);
        let silence = vec![0.0f32; 16000]; // 1s @ 16k
        let out = enhance_mono(&silence, &mut session, 0.0).expect("enhance");
        assert_eq!(out.len(), silence.len());
    }
}

fn write_wav_pcm16(path: &Path, samples: &[f32], sample_rate: u32) -> Result<(), DpdfError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w =
        hound::WavWriter::create(path, spec).map_err(|e| DpdfError::Io(std::io::Error::other(e)))?;
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        w.write_sample(v)
            .map_err(|e| DpdfError::Io(std::io::Error::other(e)))?;
    }
    w.finalize()
        .map_err(|e| DpdfError::Io(std::io::Error::other(e)))?;
    Ok(())
}
