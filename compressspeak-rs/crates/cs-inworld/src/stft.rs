//! STFT / iSTFT matching Inworld Voice Isolator / torch.stft(center=True).
//! n_fft=2048, hop=480, periodic Hann, reflect pad when length > n_fft/2.

use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::Arc;

pub const N_FFT: usize = 2048;
pub const HOP: usize = 480;
pub const BINS: usize = N_FFT / 2 + 1; // 1025
pub const SR: u32 = 48_000;
pub const CHUNK: usize = 8 * SR as usize;
pub const OVERLAP: usize = 2 * SR as usize;

/// Periodic Hann: 0.5 - 0.5 * cos(2π n / N)
pub fn periodic_hann(n_fft: usize) -> Vec<f32> {
    let n = n_fft as f32;
    (0..n_fft)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n).cos())
        .collect()
}

pub fn frame_count(length: usize) -> usize {
    1 + length / HOP
}

fn reflect_at(x: &[f32], i: isize) -> f32 {
    let l = x.len() as isize;
    let pad = (N_FFT / 2) as isize;
    let j = i - pad;
    if j >= 0 && j < l {
        return x[j as usize];
    }
    if l <= pad {
        return 0.0;
    }
    if j < 0 {
        x[(-j) as usize]
    } else {
        // j >= L : mirror from end → index 2L-2-j
        let idx = 2 * l - 2 - j;
        if idx >= 0 && idx < l {
            x[idx as usize]
        } else {
            0.0
        }
    }
}

pub fn window_starts(n: usize) -> Vec<usize> {
    if n <= CHUNK {
        return vec![0];
    }
    let mut starts = Vec::new();
    let step = CHUNK - OVERLAP;
    let mut s = 0;
    while s < n - CHUNK {
        starts.push(s);
        s += step;
    }
    starts.push(n - CHUNK);
    starts
}

/// Overlapping 8 s windows with 2 s linear crossfade (VoiceIsolatorNetwork.enhance).
pub fn enhance_chunks<F>(audio: &[f32], mut isolate_window: F) -> Result<Vec<f32>, String>
where
    F: FnMut(&[f32]) -> Result<Vec<f32>, String>,
{
    let n = audio.len();
    let starts = window_starts(n);
    if starts.len() == 1 {
        let chunk = if n <= CHUNK {
            audio.to_vec()
        } else {
            audio[..CHUNK.min(n)].to_vec()
        };
        let mut y = isolate_window(&chunk)?;
        y.truncate(n);
        if y.len() < n {
            y.resize(n, 0.0);
        }
        return Ok(y);
    }

    let mut out = vec![0f32; n];
    let mut weight = vec![0f32; n];
    for (index, &start) in starts.iter().enumerate() {
        let end = start + CHUNK;
        let mut w = vec![1f32; CHUNK];
        if index > 0 {
            let prev_end = starts[index - 1] + CHUNK;
            let fade_in = prev_end.saturating_sub(OVERLAP).saturating_sub(start);
            for i in 0..fade_in.min(CHUNK) {
                w[i] = 0.0;
            }
            for i in 0..OVERLAP {
                let idx = fade_in + i;
                if idx < CHUNK {
                    w[idx] = i as f32 / (OVERLAP - 1) as f32;
                }
            }
        }
        if end < n {
            for i in 0..OVERLAP {
                w[CHUNK - OVERLAP + i] = 1.0 - (i as f32 / (OVERLAP - 1) as f32);
            }
        }
        let y = isolate_window(&audio[start..end])?;
        for i in 0..CHUNK {
            out[start + i] += y[i] * w[i];
            weight[start + i] += w[i];
        }
    }
    for i in 0..n {
        out[i] /= weight[i].max(1e-6);
    }
    Ok(out)
}

/// Keep FFT planner for repeated window runs.
pub struct StftKit {
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
}

impl StftKit {
    pub fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        Self {
            fft: planner.plan_fft_forward(N_FFT),
            ifft: planner.plan_fft_inverse(N_FFT),
            window: periodic_hann(N_FFT),
        }
    }

    pub fn stft(&self, x: &[f32]) -> (Vec<f32>, usize) {
        let t_frames = frame_count(x.len());
        let mut out = vec![0f32; t_frames * BINS * 2];
        let mut buf = vec![Complex32::new(0.0, 0.0); N_FFT];
        for t in 0..t_frames {
            let o = (t * HOP) as isize;
            for n in 0..N_FFT {
                buf[n] = Complex32::new(reflect_at(x, o + n as isize) * self.window[n], 0.0);
            }
            self.fft.process(&mut buf);
            let base = t * BINS * 2;
            for k in 0..BINS {
                out[base + 2 * k] = buf[k].re;
                out[base + 2 * k + 1] = buf[k].im;
            }
        }
        (out, t_frames)
    }

    pub fn istft(&self, spec: &[f32], t_frames: usize, length: usize) -> Vec<f32> {
        let total = N_FFT + HOP * (t_frames.saturating_sub(1));
        let mut acc = vec![0f32; total];
        let mut env = vec![0f32; total];
        let mut buf = vec![Complex32::new(0.0, 0.0); N_FFT];
        let scale = 1.0 / N_FFT as f32;

        for t in 0..t_frames {
            let base = t * BINS * 2;
            for k in 0..BINS {
                let re = spec[base + 2 * k];
                let mut im = spec[base + 2 * k + 1];
                if k == 0 || k == BINS - 1 {
                    im = 0.0;
                }
                buf[k] = Complex32::new(re, im);
            }
            for k in BINS..N_FFT {
                buf[k] = buf[N_FFT - k].conj();
            }
            self.ifft.process(&mut buf);
            let o = t * HOP;
            for n in 0..N_FFT {
                let w = self.window[n];
                acc[o + n] += buf[n].re * scale * w;
                env[o + n] += w * w;
            }
        }

        let start = N_FFT / 2;
        let mut y = vec![0f32; length];
        for i in 0..length {
            if start + i >= total {
                break;
            }
            let e = env[start + i];
            y[i] = if e > 1e-11 { acc[start + i] / e } else { 0.0 };
        }
        y
    }
}

impl Default for StftKit {
    fn default() -> Self {
        Self::new()
    }
}
