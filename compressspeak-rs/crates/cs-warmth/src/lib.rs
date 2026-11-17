//! Vocal warmth DSP (Pedalboard replacement): high-shelf + soft distortion + compressor + gain.

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WarmthError {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("WAV: {0}")]
    Wav(String),
    #[error("décodage audio: {0}")]
    Decode(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmthParams {
    pub high_shelf_gain_db: f32,
    pub high_shelf_cutoff_hz: f32,
    pub distortion_drive_db: f32,
    pub compressor_threshold_db: f32,
    pub compressor_ratio: f32,
    pub final_gain_db: f32,
    /// Presence / air après DPDF (réouvre le haut du spectre).
    #[serde(default)]
    pub unmuffle_enabled: bool,
    #[serde(default = "default_unmuffle_gain")]
    pub unmuffle_gain_db: f32,
    #[serde(default = "default_unmuffle_cutoff")]
    pub unmuffle_cutoff_hz: f32,
}

fn default_unmuffle_gain() -> f32 {
    4.0
}
fn default_unmuffle_cutoff() -> f32 {
    3500.0
}

impl Default for WarmthParams {
    fn default() -> Self {
        Self {
            high_shelf_gain_db: 2.0,
            high_shelf_cutoff_hz: 400.0,
            distortion_drive_db: 3.0,
            compressor_threshold_db: -15.0,
            compressor_ratio: 3.0,
            final_gain_db: 2.0,
            unmuffle_enabled: true,
            unmuffle_gain_db: 4.0,
            unmuffle_cutoff_hz: 3500.0,
        }
    }
}

impl WarmthParams {
    pub fn preset_subtle() -> Self {
        Self {
            high_shelf_gain_db: 1.0,
            high_shelf_cutoff_hz: 450.0,
            distortion_drive_db: 1.5,
            compressor_threshold_db: -18.0,
            compressor_ratio: 2.0,
            final_gain_db: 1.0,
            unmuffle_enabled: true,
            unmuffle_gain_db: 3.0,
            unmuffle_cutoff_hz: 4000.0,
        }
    }

    pub fn preset_standard() -> Self {
        Self::default()
    }

    pub fn preset_radio() -> Self {
        Self {
            high_shelf_gain_db: 3.0,
            high_shelf_cutoff_hz: 350.0,
            distortion_drive_db: 5.0,
            compressor_threshold_db: -12.0,
            compressor_ratio: 4.0,
            final_gain_db: 3.0,
            unmuffle_enabled: true,
            unmuffle_gain_db: 5.0,
            unmuffle_cutoff_hz: 3200.0,
        }
    }

    pub fn preset_podcast() -> Self {
        Self {
            high_shelf_gain_db: 2.5,
            high_shelf_cutoff_hz: 380.0,
            distortion_drive_db: 2.5,
            compressor_threshold_db: -16.0,
            compressor_ratio: 3.0,
            final_gain_db: 2.0,
            unmuffle_enabled: true,
            unmuffle_gain_db: 4.0,
            unmuffle_cutoff_hz: 3500.0,
        }
    }
}

fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// RBJ high-shelf biquad.
struct HighShelf {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl HighShelf {
    fn new(sample_rate: f32, cutoff_hz: f32, gain_db: f32, q: f32) -> Self {
        let a = db_to_lin(gain_db);
        let w0 = 2.0 * PI * cutoff_hz / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

struct SoftDistortion {
    drive: f32,
}

impl SoftDistortion {
    fn new(drive_db: f32) -> Self {
        Self {
            drive: db_to_lin(drive_db),
        }
    }

    fn process(&self, x: f32) -> f32 {
        let driven = (x * self.drive).tanh();
        // Compensate approximate loudness
        driven / self.drive.tanh().max(0.1)
    }
}

struct Compressor {
    threshold: f32,
    ratio: f32,
    env: f32,
    attack: f32,
    release: f32,
}

impl Compressor {
    fn new(sample_rate: f32, threshold_db: f32, ratio: f32) -> Self {
        Self {
            threshold: db_to_lin(threshold_db),
            ratio,
            env: 0.0,
            attack: (-1.0 / (0.01 * sample_rate)).exp(),
            release: (-1.0 / (0.1 * sample_rate)).exp(),
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let abs = x.abs();
        let coeff = if abs > self.env {
            self.attack
        } else {
            self.release
        };
        self.env = coeff * self.env + (1.0 - coeff) * abs;
        if self.env <= self.threshold || self.threshold <= 0.0 {
            return x;
        }
        let over = self.env / self.threshold;
        let gain = over.powf(1.0 / self.ratio - 1.0);
        x * gain
    }
}

fn process_interleaved(samples: &mut [f32], channels: usize, sample_rate: f32, params: &WarmthParams) {
    let mut shelves: Vec<HighShelf> = (0..channels)
        .map(|_| HighShelf::new(sample_rate, params.high_shelf_cutoff_hz, params.high_shelf_gain_db, 1.0))
        .collect();
    let dist = SoftDistortion::new(params.distortion_drive_db);
    let mut comps: Vec<Compressor> = (0..channels)
        .map(|_| Compressor::new(sample_rate, params.compressor_threshold_db, params.compressor_ratio))
        .collect();
    // Désourdage : high-shelf aiguës (présence) pour compenser DPDF
    let mut unmuffles: Vec<Option<HighShelf>> = (0..channels)
        .map(|_| {
            if params.unmuffle_enabled && params.unmuffle_gain_db.abs() > 0.05 {
                Some(HighShelf::new(
                    sample_rate,
                    params.unmuffle_cutoff_hz.max(800.0),
                    params.unmuffle_gain_db,
                    0.7,
                ))
            } else {
                None
            }
        })
        .collect();
    let final_gain = db_to_lin(params.final_gain_db);

    let frames = samples.len() / channels;
    for i in 0..frames {
        for c in 0..channels {
            let idx = i * channels + c;
            let mut s = samples[idx];
            s = shelves[c].process(s);
            s = dist.process(s);
            s = comps[c].process(s);
            if let Some(ref mut u) = unmuffles[c] {
                s = u.process(s);
            }
            samples[idx] = (s * final_gain).clamp(-1.0, 1.0);
        }
    }
}

fn load_wav_f32(path: &Path) -> Result<(Vec<f32>, u32, u16), WarmthError> {
    let mut reader = WavReader::open(path).map_err(|e| WarmthError::Wav(e.to_string()))?;
    let spec = reader.spec();
    let samples: Result<Vec<f32>, _> = match spec.sample_format {
        SampleFormat::Float => reader.samples::<f32>().collect(),
        SampleFormat::Int => reader
            .samples::<i32>()
            .map(|s| {
                s.map(|v| {
                    let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
                    v as f32 / max
                })
            })
            .collect(),
    };
    let samples = samples.map_err(|e| WarmthError::Wav(e.to_string()))?;
    Ok((samples, spec.sample_rate, spec.channels))
}

fn write_wav_f32(path: &Path, samples: &[f32], sample_rate: u32, channels: u16) -> Result<(), WarmthError> {
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).map_err(|e| WarmthError::Wav(e.to_string()))?;
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer.write_sample(v).map_err(|e| WarmthError::Wav(e.to_string()))?;
    }
    writer.finalize().map_err(|e| WarmthError::Wav(e.to_string()))?;
    Ok(())
}

/// Decode any format via symphonia to f32 interleaved, then write WAV for processing if needed.
pub fn decode_to_f32(path: &Path) -> Result<(Vec<f32>, u32, u16), WarmthError> {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("wav"))
        .unwrap_or(false)
    {
        return load_wav_f32(path);
    }
    decode_with_symphonia(path)
}

fn decode_with_symphonia(path: &Path) -> Result<(Vec<f32>, u32, u16), WarmthError> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| WarmthError::Decode(e.to_string()))?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| WarmthError::Decode("pas de piste audio".into()))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| WarmthError::Decode(e.to_string()))?;
    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let mut channels = track.codec_params.channels.map(|c| c.count() as u16).unwrap_or(1);
    let mut out = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::ResetRequired) => continue,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = decoder
            .decode(&packet)
            .map_err(|e| WarmthError::Decode(e.to_string()))?;
        sample_rate = decoded.spec().rate;
        channels = decoded.spec().channels.count() as u16;
        if sample_buf.is_none() {
            sample_buf = Some(SampleBuffer::new(decoded.capacity() as u64, *decoded.spec()));
        }
        if let Some(buf) = sample_buf.as_mut() {
            buf.copy_interleaved_ref(decoded);
            out.extend_from_slice(buf.samples());
        }
    }
    Ok((out, sample_rate, channels))
}

pub fn apply_warmth(input: &Path, output: &Path, params: &WarmthParams) -> Result<(), WarmthError> {
    tracing::info!(
        "Mixage{}: {}",
        if params.unmuffle_enabled {
            " + désourdage"
        } else {
            ""
        },
        input.display()
    );
    let (mut samples, sr, ch) = decode_to_f32(input)?;
    if samples.is_empty() {
        return Err(WarmthError::Decode("audio vide".into()));
    }
    process_interleaved(&mut samples, ch as usize, sr as f32, params);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_wav_f32(output, &samples, sr, ch)?;
    tracing::info!("Warmth OK: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_silence() {
        let mut s = vec![0.0f32; 100];
        process_interleaved(&mut s, 1, 44100.0, &WarmthParams::default());
        assert!(s.iter().all(|x| x.abs() < 1e-3));
    }
}
