//! STFT / iSTFT for UVR MDX-Net (periodic Hann, centered reflect pad).

use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::Arc;

pub const HOP: usize = 1024;
pub const RATE: u32 = 44_100;
pub const DIM_T: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MdxParams {
    pub dim_f: usize,
    pub dim_t: usize,
    pub n_fft: usize,
    pub compensate: f32,
    /// Model outputs instrumental: vocals = mix − output × compensate.
    pub instrumental: bool,
}

impl MdxParams {
    pub fn chunk_samples(self) -> usize {
        HOP * (self.dim_t - 1)
    }

    pub fn trim(self) -> usize {
        self.n_fft / 2
    }

    pub fn step(self) -> usize {
        self.chunk_samples() - 2 * self.trim()
    }
}

pub struct Stft {
    p: MdxParams,
    win: Vec<f32>,
    fwd: Arc<dyn Fft<f32>>,
    inv: Arc<dyn Fft<f32>>,
}

impl Stft {
    pub fn new(p: MdxParams) -> Self {
        let n = p.n_fft;
        let win = (0..n)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())
            .collect();
        let mut planner = FftPlanner::<f32>::new();
        Self {
            p,
            win,
            fwd: planner.plan_fft_forward(n),
            inv: planner.plan_fft_inverse(n),
        }
    }

    /// One channel chunk → `frames` × `dim_f` complex bins.
    pub fn forward(&self, x: &[f32]) -> Vec<Vec<Complex32>> {
        let (n, half) = (self.p.n_fft, self.p.n_fft / 2);
        let len = x.len() as isize;
        let at = |i: isize| -> f32 {
            let i = i - half as isize;
            let j = if i < 0 {
                -i
            } else if i >= len {
                2 * (len - 1) - i
            } else {
                i
            };
            x[j.clamp(0, len - 1) as usize]
        };
        let frames = x.len() / HOP + 1;
        let mut buf = vec![Complex32::default(); n];
        (0..frames)
            .map(|t| {
                for (k, b) in buf.iter_mut().enumerate() {
                    *b = Complex32::new(at((t * HOP + k) as isize) * self.win[k], 0.0);
                }
                self.fwd.process(&mut buf);
                buf[..self.p.dim_f].to_vec()
            })
            .collect()
    }

    /// Frames × `dim_f` → `len` samples (OLA + center crop).
    pub fn inverse(&self, spec: &[Vec<Complex32>], len: usize) -> Vec<f32> {
        let (n, half) = (self.p.n_fft, self.p.n_fft / 2);
        let total = n + HOP * (spec.len().saturating_sub(1));
        let mut out = vec![0f32; total];
        let mut wsum = vec![0f32; total];
        let mut buf = vec![Complex32::default(); n];
        for (t, frame) in spec.iter().enumerate() {
            buf.fill(Complex32::default());
            for (k, &v) in frame.iter().enumerate().take(half + 1) {
                buf[k] = v;
            }
            buf[0].im = 0.0;
            if half < n {
                buf[half].im = 0.0;
            }
            for k in 1..half {
                buf[n - k] = buf[k].conj();
            }
            self.inv.process(&mut buf);
            let o = t * HOP;
            for k in 0..n {
                out[o + k] += buf[k].re / n as f32 * self.win[k];
                wsum[o + k] += self.win[k] * self.win[k];
            }
        }
        (0..len)
            .map(|i| {
                let w = wsum.get(i + half).copied().unwrap_or(0.0);
                if w > 1e-8 {
                    out[i + half] / w
                } else {
                    0.0
                }
            })
            .collect()
    }
}
