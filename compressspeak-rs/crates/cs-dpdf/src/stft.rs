//! Causal STFT / ISTFT (Vorbis window, 50% hop) — même famille que HushMic / DPDFNet.

use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::Arc;

pub fn vorbis_window(n_fft: usize) -> Vec<f32> {
    let h = (n_fft as f32) / 2.0;
    (0..n_fft)
        .map(|n| {
            let s = (0.5 * std::f32::consts::PI * (n as f32 + 0.5) / h).sin();
            (0.5 * std::f32::consts::PI * s * s).sin()
        })
        .collect()
}

pub struct Analysis {
    n_fft: usize,
    hop: usize,
    freq_bins: usize,
    window: Vec<f32>,
    ring: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex32>,
}

impl Analysis {
    pub fn new(n_fft: usize) -> Self {
        let hop = n_fft / 2;
        let freq_bins = n_fft / 2 + 1;
        let mut planner = FftPlanner::<f32>::new();
        Self {
            n_fft,
            hop,
            freq_bins,
            window: vorbis_window(n_fft),
            ring: vec![0.0; n_fft],
            fft: planner.plan_fft_forward(n_fft),
            scratch: vec![Complex32::new(0.0, 0.0); n_fft],
        }
    }

    pub fn push_hop(&mut self, in_hop: &[f32], out_spec: &mut [f32]) {
        debug_assert_eq!(in_hop.len(), self.hop);
        debug_assert_eq!(out_spec.len(), self.freq_bins * 2);
        self.ring.copy_within(self.hop.., 0);
        self.ring[self.n_fft - self.hop..].copy_from_slice(in_hop);
        for i in 0..self.n_fft {
            self.scratch[i] = Complex32::new(self.ring[i] * self.window[i], 0.0);
        }
        self.fft.process(&mut self.scratch);
        for k in 0..self.freq_bins {
            out_spec[2 * k] = self.scratch[k].re;
            out_spec[2 * k + 1] = self.scratch[k].im;
        }
    }
}

pub struct Synthesis {
    n_fft: usize,
    hop: usize,
    freq_bins: usize,
    window: Vec<f32>,
    ola: Vec<f32>,
    ifft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex32>,
}

impl Synthesis {
    pub fn new(n_fft: usize) -> Self {
        let hop = n_fft / 2;
        let freq_bins = n_fft / 2 + 1;
        let mut planner = FftPlanner::<f32>::new();
        Self {
            n_fft,
            hop,
            freq_bins,
            window: vorbis_window(n_fft),
            ola: vec![0.0; n_fft],
            ifft: planner.plan_fft_inverse(n_fft),
            scratch: vec![Complex32::new(0.0, 0.0); n_fft],
        }
    }

    pub fn add_frame(&mut self, spec: &[f32], out_hop: &mut [f32]) {
        debug_assert_eq!(spec.len(), self.freq_bins * 2);
        debug_assert_eq!(out_hop.len(), self.hop);
        for k in 0..self.freq_bins {
            self.scratch[k] = Complex32::new(spec[2 * k], spec[2 * k + 1]);
        }
        for j in self.freq_bins..self.n_fft {
            self.scratch[j] = self.scratch[self.n_fft - j].conj();
        }
        self.ifft.process(&mut self.scratch);
        let norm = 1.0 / (self.n_fft as f32);
        self.ola.copy_within(self.hop.., 0);
        for s in &mut self.ola[self.n_fft - self.hop..] {
            *s = 0.0;
        }
        for i in 0..self.n_fft {
            self.ola[i] += self.scratch[i].re * norm * self.window[i];
        }
        out_hop.copy_from_slice(&self.ola[..self.hop]);
    }
}
