//! ONNX Runtime session for Ceva DPDFNet streaming models.

use ort::environment::Environment;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;
use std::sync::{Arc, OnceLock};

static ENV: OnceLock<Arc<Environment>> = OnceLock::new();

fn environment() -> Result<&'static Arc<Environment>, String> {
    if let Some(env) = ENV.get() {
        return Ok(env);
    }
    let env = ort::init()
        .with_name("compressspeak-dpdf")
        .build()
        .map_err(|e| e.to_string())?;
    let _ = ENV.set(Arc::new(env));
    ENV.get().ok_or_else(|| "ORT env".into())
}

pub struct DpdfSession {
    session: Session,
    pub state_size: usize,
    pub init_state: Vec<f32>,
    pub freq_bins: usize,
    pub n_fft: usize,
    in_spec: String,
    in_state: String,
    out_spec: String,
    out_state: String,
}

impl DpdfSession {
    pub fn load(path: &Path) -> Result<Self, String> {
        let env = environment()?;
        let session = Session::builder(env)
            .map_err(|e| e.to_string())?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?
            .with_intra_threads(1)
            .map_err(|e| e.to_string())?
            .commit_from_file(path)
            .map_err(|e| format!("ONNX {}: {e}", path.display()))?;

        let inputs = session.inputs();
        let outputs = session.outputs();
        if inputs.len() < 2 || outputs.len() < 2 {
            return Err("modèle ONNX: 2 entrées / 2 sorties attendues".into());
        }
        let in_spec = inputs[0].name().to_string();
        let in_state = inputs[1].name().to_string();
        let out_spec = outputs[0].name().to_string();
        let out_state = outputs[1].name().to_string();

        let freq_bins = inputs[0]
            .dtype()
            .tensor_shape()
            .and_then(|s| s.get(2).copied())
            .filter(|&d| d > 0)
            .unwrap_or(161) as usize;
        let n_fft = (freq_bins - 1) * 2;

        let meta = session.metadata().map_err(|e| e.to_string())?;
        let state_size: usize = meta
            .custom("state_size")
            .and_then(|s| s.trim().parse().ok())
            .or_else(|| {
                inputs[1]
                    .dtype()
                    .tensor_shape()
                    .and_then(|s| s.last().copied())
                    .filter(|&d| d > 0)
                    .map(|d| d as usize)
            })
            .ok_or_else(|| "state_size introuvable".to_string())?;

        let erb_sz: usize = meta
            .custom("erb_norm_state_size")
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let spec_sz: usize = meta
            .custom("spec_norm_state_size")
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let erb_init = meta
            .custom("erb_norm_init")
            .map(|s| parse_csv(&s))
            .unwrap_or_default();
        let spec_init = meta
            .custom("spec_norm_init")
            .map(|s| parse_csv(&s))
            .unwrap_or_default();
        drop(meta);

        let mut init_state = vec![0f32; state_size];
        if erb_init.len() == erb_sz && erb_sz <= state_size {
            init_state[..erb_sz].copy_from_slice(&erb_init);
        }
        if spec_init.len() == spec_sz && erb_sz + spec_sz <= state_size {
            init_state[erb_sz..erb_sz + spec_sz].copy_from_slice(&spec_init);
        }

        Ok(Self {
            session,
            state_size,
            init_state,
            freq_bins,
            n_fft,
            in_spec,
            in_state,
            out_spec,
            out_state,
        })
    }

    pub fn run_frame(
        &mut self,
        spec: &[f32],
        state_in: &[f32],
        spec_e: &mut [f32],
        state_out: &mut Vec<f32>,
    ) -> Result<(), String> {
        let spec_len = self.freq_bins * 2;
        if spec.len() != spec_len || spec_e.len() != spec_len {
            return Err(format!(
                "spec len {} != expected {}",
                spec.len(),
                spec_len
            ));
        }
        let spec_t = TensorRef::from_array_view(([1usize, 1, self.freq_bins, 2], spec))
            .map_err(|e| e.to_string())?;
        let state_t =
            TensorRef::from_array_view(([state_in.len()], state_in)).map_err(|e| e.to_string())?;

        let outputs = self
            .session
            .run(ort::inputs![
                self.in_spec.as_str() => spec_t,
                self.in_state.as_str() => state_t,
            ])
            .map_err(|e| e.to_string())?;

        let (_, e_slice) = outputs
            .get(self.out_spec.as_str())
            .ok_or_else(|| format!("sortie '{}' absente", self.out_spec))?
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        if e_slice.len() != spec_e.len() {
            return Err(format!(
                "spec_e len {} != {}",
                e_slice.len(),
                spec_e.len()
            ));
        }
        spec_e.copy_from_slice(e_slice);

        let (_, s_slice) = outputs
            .get(self.out_state.as_str())
            .ok_or_else(|| format!("sortie '{}' absente", self.out_state))?
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        state_out.clear();
        state_out.extend_from_slice(s_slice);
        Ok(())
    }
}

fn parse_csv(s: &str) -> Vec<f32> {
    s.split(',')
        .filter_map(|t| t.trim().parse().ok())
        .collect()
}

/// Enhance mono f32 at model sample rate; returns same-length mono.
pub fn enhance_mono(
    samples: &[f32],
    session: &mut DpdfSession,
    attn_limit_db: f32,
) -> Result<Vec<f32>, String> {
    use crate::stft::{Analysis, Synthesis};

    let n_fft = session.n_fft;
    let hop = n_fft / 2;
    let spec_len = session.freq_bins * 2;
    let mut ana = Analysis::new(n_fft);
    let mut syn = Synthesis::new(n_fft);

    let mut state = session.init_state.clone();
    let mut state_out = vec![0f32; session.state_size];
    let mut spec = vec![0f32; spec_len];
    let mut spec_e = vec![0f32; spec_len];
    let mut hop_out = vec![0f32; hop];

    let mut input = samples.to_vec();
    let rem = input.len() % hop;
    if rem != 0 {
        input.extend(std::iter::repeat_n(0.0, hop - rem));
    }
    let latency_hops = 5;
    input.extend(std::iter::repeat_n(0.0, hop * latency_hops));

    let mut out = Vec::with_capacity(input.len());
    let hops = input.len() / hop;
    let alpha = if attn_limit_db > 0.0 {
        10f32.powf(-attn_limit_db / 20.0)
    } else {
        0.0
    };

    for h in 0..hops {
        let hop_in = &input[h * hop..(h + 1) * hop];
        ana.push_hop(hop_in, &mut spec);
        session.run_frame(&spec, &state, &mut spec_e, &mut state_out)?;
        if alpha > 0.0 {
            for i in 0..spec_len {
                spec_e[i] = alpha * spec[i] + (1.0 - alpha) * spec_e[i];
            }
        }
        syn.add_frame(&spec_e, &mut hop_out);
        out.extend_from_slice(&hop_out);
        state.copy_from_slice(&state_out);
    }

    let delay = hop * latency_hops;
    if out.len() > delay + samples.len() {
        Ok(out[delay..delay + samples.len()].to_vec())
    } else if out.len() >= samples.len() {
        Ok(out[..samples.len()].to_vec())
    } else {
        let mut padded = out;
        padded.resize(samples.len(), 0.0);
        Ok(padded)
    }
}
