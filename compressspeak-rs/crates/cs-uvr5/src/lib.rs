//! UVR MDX-Net — séparation vocale native via ONNX Runtime (pas de Python).
//! Modèles : HuggingFace `aaatmy/uvr-mdx-net-onnx` (copies UVR officielles).

mod engine;
mod stft;

use cs_ffmpeg::{build_stereo_wav_command, resolve_ffmpeg, run_command};
use engine::MdxSession;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use stft::{MdxParams, DIM_T, RATE};
use thiserror::Error;

pub use stft::RATE as SAMPLE_RATE;

#[derive(Debug, Error)]
pub enum Uvr5Error {
    #[error("UVR MDX: {0}")]
    Failed(String),
    #[error("FFmpeg: {0}")]
    Ffmpeg(String),
    #[error("téléchargement modèle: {0}")]
    Download(String),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
}

pub const DEFAULT_MODEL: &str = "UVR-MDX-NET-Voc_FT.onnx";

pub const AVAILABLE_MODELS: &[&str] = &[
    "UVR-MDX-NET-Voc_FT.onnx",
    "Kim_Vocal_2.onnx",
    "UVR-MDX-NET-Inst_HQ_4.onnx",
];

pub const AVAILABLE_STEMS: &[&str] = &["Vocals", "Instrumental"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Uvr5Params {
    /// Conservé pour compat config (plus utilisé — natif ONNX).
    #[serde(default)]
    pub path: String,
    /// Nom du fichier modèle ONNX.
    pub model: String,
    /// Conservé pour compat ; le cache natif est `models_dir()`.
    #[serde(default)]
    pub model_dir: String,
    pub stem: String,
    #[serde(default)]
    pub chunk_sec: u32,
    #[serde(default)]
    pub mdx_segment_size: u32,
    #[serde(default)]
    pub mdxc_overlap: u32,
    #[serde(default)]
    pub mdx_batch_size: u32,
}

impl Default for Uvr5Params {
    fn default() -> Self {
        Self {
            path: String::new(),
            model: DEFAULT_MODEL.into(),
            model_dir: String::new(),
            stem: "Vocals".into(),
            chunk_sec: 1200,
            mdx_segment_size: 2048,
            mdxc_overlap: 8,
            mdx_batch_size: 1,
        }
    }
}

impl Uvr5Params {
    pub fn normalize(&mut self) {
        let name = self.model.trim();
        if name.is_empty() || name.ends_with(".ckpt") || name.ends_with(".pth") {
            self.model = DEFAULT_MODEL.into();
        } else if !AVAILABLE_MODELS.iter().any(|m| *m == name) {
            // Keep custom .onnx if user set one, else reset.
            if !name.ends_with(".onnx") {
                self.model = DEFAULT_MODEL.into();
            }
        }
        if self.stem != "Vocals" && self.stem != "Instrumental" {
            self.stem = "Vocals".into();
        }
    }

    pub fn model_file(&self) -> &str {
        let n = self.model.trim();
        if n.is_empty() {
            DEFAULT_MODEL
        } else {
            n
        }
    }
}

fn model_spec(filename: &str) -> MdxParams {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);
    match stem {
        "Kim_Vocal_2" | "Kim_Vocal_1" => MdxParams {
            dim_f: 3072,
            dim_t: DIM_T,
            n_fft: 7680,
            compensate: 1.009,
            instrumental: false,
        },
        "UVR-MDX-NET-Inst_HQ_4" | "UVR-MDX-NET-Inst_HQ_5" => MdxParams {
            dim_f: 2560,
            dim_t: DIM_T,
            n_fft: 5120,
            compensate: 1.019,
            instrumental: true,
        },
        "UVR-MDX-NET-Inst_HQ_3" => MdxParams {
            dim_f: 3072,
            dim_t: DIM_T,
            n_fft: 6144,
            compensate: 1.022,
            instrumental: true,
        },
        // UVR-MDX-NET-Voc_FT and default vocal models
        _ => MdxParams {
            dim_f: 3072,
            dim_t: DIM_T,
            n_fft: 7680,
            compensate: 1.021,
            instrumental: false,
        },
    }
}

fn hf_url(filename: &str) -> String {
    format!("https://huggingface.co/aaatmy/uvr-mdx-net-onnx/resolve/main/{filename}")
}

pub fn models_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "CompressSpeak", "Compress Speak")
        .map(|d| d.data_local_dir().join("models").join("uvr"))
        .unwrap_or_else(|| PathBuf::from("models/uvr"))
}

pub fn find_local_model(filename: &str) -> Option<PathBuf> {
    let p = models_dir().join(filename);
    if p.is_file() && p.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false) {
        Some(p)
    } else {
        None
    }
}

pub fn model_is_ready(filename: &str) -> bool {
    find_local_model(filename).is_some()
}

/// Toujours disponible (ORT embarqué ; modèle téléchargé au besoin).
pub fn is_available(_params: &Uvr5Params) -> bool {
    true
}

pub fn ensure_model(filename: &str) -> Result<PathBuf, Uvr5Error> {
    if let Some(p) = find_local_model(filename) {
        return Ok(p);
    }
    let dest_dir = models_dir();
    std::fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(filename);
    if dest.is_file() && dest.metadata()?.len() > 1_000_000 {
        return Ok(dest);
    }

    tracing::info!("Téléchargement UVR MDX {filename}…");
    let resp = ureq::get(&hf_url(filename))
        .call()
        .map_err(|e| Uvr5Error::Download(e.to_string()))?;
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
        if total < 1_000_000 {
            let _ = std::fs::remove_file(&tmp);
            return Err(Uvr5Error::Download(format!(
                "fichier trop petit ({total} octets)"
            )));
        }
    }
    std::fs::rename(&tmp, &dest)?;
    Ok(dest)
}

static SESSION_CACHE: OnceLock<Mutex<Option<(String, MdxSession)>>> = OnceLock::new();

fn session_slot() -> &'static Mutex<Option<(String, MdxSession)>> {
    SESSION_CACHE.get_or_init(|| Mutex::new(None))
}

fn with_session<R>(
    filename: &str,
    f: impl FnOnce(&mut MdxSession) -> Result<R, Uvr5Error>,
) -> Result<R, Uvr5Error> {
    let path = ensure_model(filename)?;
    let mut slot = session_slot().lock();
    let need_load = match slot.as_ref() {
        Some((name, _)) => name != filename,
        None => true,
    };
    if need_load {
        let params = model_spec(filename);
        let s = MdxSession::load(&path, params).map_err(Uvr5Error::Failed)?;
        *slot = Some((filename.to_string(), s));
    }
    let session = slot
        .as_mut()
        .map(|(_, s)| s)
        .ok_or_else(|| Uvr5Error::Failed("session absente".into()))?;
    f(session)
}

/// Run vocal isolation; returns path to primary stem WAV (mono mixdown of stereo stem).
pub fn separate_vocals(
    input: &Path,
    output_dir: &Path,
    params: &Uvr5Params,
    data_dir: Option<&Path>,
) -> Result<PathBuf, Uvr5Error> {
    let mut params = params.clone();
    params.normalize();
    let model = params.model_file().to_string();
    let want_vocals = params.stem.eq_ignore_ascii_case("Vocals");

    std::fs::create_dir_all(output_dir)?;
    let ffmpeg = resolve_ffmpeg(data_dir).map_err(|e| Uvr5Error::Ffmpeg(e.to_string()))?;
    let wav_in = output_dir.join("uvr_input_44100.wav");
    let cmd = build_stereo_wav_command(&ffmpeg, input, &wav_in, RATE);
    run_command(&cmd).map_err(|e| Uvr5Error::Ffmpeg(e.to_string()))?;

    let (mix, sr) = read_wav_stereo_f32(&wav_in)?;
    if sr != RATE {
        return Err(Uvr5Error::Failed(format!(
            "sample rate {sr} != {RATE} après FFmpeg"
        )));
    }

    let primary = with_session(&model, |session| {
        session.separate(&mix).map_err(Uvr5Error::Failed)
    })?;

    // `separate` always returns vocals (instrumental models are flipped inside).
    let stem_ch: [Vec<f32>; 2] = if want_vocals {
        primary
    } else {
        [
            mix[0]
                .iter()
                .zip(&primary[0])
                .map(|(m, v)| m - v)
                .collect(),
            mix[1]
                .iter()
                .zip(&primary[1])
                .map(|(m, v)| m - v)
                .collect(),
        ]
    };

    // Mono mixdown for the rest of the CompressSpeak pipeline
    let n = stem_ch[0].len().min(stem_ch[1].len());
    let mut mono = Vec::with_capacity(n);
    for i in 0..n {
        mono.push(0.5 * (stem_ch[0][i] + stem_ch[1][i]));
    }

    let out = output_dir.join(format!(
        "{}_{}.wav",
        input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "stem".into()),
        if want_vocals { "Vocals" } else { "Instrumental" }
    ));
    write_wav_pcm16(&out, &mono, RATE)?;
    tracing::info!("UVR MDX ONNX OK ({model}) → {}", out.display());
    Ok(out)
}

fn read_wav_stereo_f32(path: &Path) -> Result<([Vec<f32>; 2], u32), Uvr5Error> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| Uvr5Error::Io(std::io::Error::other(e)))?;
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
    if ch == 0 {
        return Err(Uvr5Error::Failed("WAV sans canaux".into()));
    }
    if ch == 1 {
        return Ok(([samples.clone(), samples], sr));
    }
    let frames = samples.len() / ch;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for i in 0..frames {
        left.push(samples[i * ch]);
        right.push(samples[i * ch + 1.min(ch - 1)]);
    }
    Ok(([left, right], sr))
}

fn write_wav_pcm16(path: &Path, samples: &[f32], sr: u32) -> Result<(), Uvr5Error> {
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
        hound::WavWriter::create(path, spec).map_err(|e| Uvr5Error::Io(std::io::Error::other(e)))?;
    for &s in samples {
        let v = (s * scale).clamp(-1.0, 1.0);
        writer
            .write_sample((v * 32767.0).round() as i16)
            .map_err(|e| Uvr5Error::Io(std::io::Error::other(e)))?;
    }
    writer
        .finalize()
        .map_err(|e| Uvr5Error::Io(std::io::Error::other(e)))?;
    Ok(())
}
