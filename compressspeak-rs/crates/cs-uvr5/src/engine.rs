//! ONNX Runtime session for UVR MDX-Net models.

use crate::stft::{MdxParams, Stft, DIM_T, HOP};
use ort::environment::Environment;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use rustfft::num_complex::Complex32;
use std::path::Path;
use std::sync::{Arc, OnceLock};

static ENV: OnceLock<Arc<Environment>> = OnceLock::new();

fn environment() -> Result<&'static Arc<Environment>, String> {
    if let Some(env) = ENV.get() {
        return Ok(env);
    }
    let env = ort::init()
        .with_name("compressspeak-uvr")
        .build()
        .map_err(|e| e.to_string())?;
    let _ = ENV.set(Arc::new(env));
    ENV.get().ok_or_else(|| "ORT env".into())
}

pub struct MdxSession {
    pub params: MdxParams,
    session: Session,
    in_name: String,
}

impl MdxSession {
    pub fn load(path: &Path, mut params: MdxParams) -> Result<Self, String> {
        let env = environment()?;
        let session = Session::builder(env)
            .map_err(|e| e.to_string())?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?
            .with_intra_threads(
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4),
            )
            .map_err(|e| e.to_string())?
            .commit_from_file(path)
            .map_err(|e| format!("ONNX {}: {e}", path.display()))?;

        let inputs = session.inputs();
        if inputs.is_empty() {
            return Err("modèle ONNX: entrée manquante".into());
        }
        let in_name = inputs[0].name().to_string();
        if let Some(shape) = inputs[0].dtype().tensor_shape() {
            if let Some(&f) = shape.get(2).filter(|&&f| f > 0) {
                params.dim_f = f as usize;
            }
            if let Some(&t) = shape.get(3).filter(|&&t| t > 0) {
                params.dim_t = t as usize;
            } else {
                params.dim_t = DIM_T;
            }
        }

        Ok(Self {
            params,
            session,
            in_name,
        })
    }

    /// One chunk (both channels, `chunk_samples` length) → primary stem channels.
    fn run_chunk(&mut self, stft: &Stft, left: &[f32], right: &[f32]) -> Result<[Vec<f32>; 2], String> {
        let (df, dt) = (self.params.dim_f, self.params.dim_t);
        let mut planes = vec![0f32; 4 * df * dt];
        for (c, ch) in [left, right].iter().enumerate() {
            let spec = stft.forward(ch);
            for (t, frame) in spec.iter().enumerate().take(dt) {
                for (f, v) in frame.iter().enumerate().take(df) {
                    planes[((2 * c) * df + f) * dt + t] = v.re;
                    planes[((2 * c + 1) * df + f) * dt + t] = v.im;
                }
            }
        }

        let input = Tensor::from_array(([1usize, 4, df, dt], planes)).map_err(|e| e.to_string())?;
        let outputs = self
            .session
            .run(ort::inputs![self.in_name.as_str() => input])
            .map_err(|e| e.to_string())?;
        let y: Vec<f32> = {
            let first = outputs
                .iter()
                .next()
                .ok_or_else(|| "sortie ONNX absente".to_string())?;
            let (_, slice) = first
                .1
                .try_extract_tensor::<f32>()
                .map_err(|e| e.to_string())?;
            slice.to_vec()
        };

        let len = left.len();
        let decode = |c: usize| -> Vec<f32> {
            let spec: Vec<Vec<Complex32>> = (0..dt)
                .map(|t| {
                    (0..df)
                        .map(|f| {
                            Complex32::new(
                                y[((2 * c) * df + f) * dt + t],
                                y[((2 * c + 1) * df + f) * dt + t],
                            )
                        })
                        .collect()
                })
                .collect();
            stft.inverse(&spec, len)
        };
        Ok([decode(0), decode(1)])
    }

    /// Separate stereo mix → primary stem (same length), with compensate / instrumental flip.
    pub fn separate(&mut self, mix: &[Vec<f32>; 2]) -> Result<[Vec<f32>; 2], String> {
        let p = self.params;
        let (chunk, trim, step) = (p.chunk_samples(), p.trim(), p.step());
        let n = mix[0].len().min(mix[1].len());
        if n == 0 {
            return Ok([Vec::new(), Vec::new()]);
        }
        let chunks = n.div_ceil(step).max(1);
        let padded: [Vec<f32>; 2] = std::array::from_fn(|c| {
            let mut v = vec![0f32; trim];
            v.extend_from_slice(&mix[c][..n]);
            v.resize(trim + chunks * step + trim, 0.0);
            v
        });
        let stft = Stft::new(p);
        let mut out: [Vec<f32>; 2] = [
            Vec::with_capacity(chunks * step),
            Vec::with_capacity(chunks * step),
        ];
        for i in 0..chunks {
            let o = i * step;
            let y = self.run_chunk(
                &stft,
                &padded[0][o..o + chunk],
                &padded[1][o..o + chunk],
            )?;
            for c in 0..2 {
                out[c].extend_from_slice(&y[c][trim..trim + step]);
            }
        }
        for (c, o) in out.iter_mut().enumerate() {
            o.truncate(n);
            for v in o.iter_mut() {
                *v *= p.compensate;
            }
            if p.instrumental {
                for (v, m) in o.iter_mut().zip(&mix[c][..n]) {
                    *v = m - *v;
                }
            }
        }
        let _ = HOP; // keep hop visible for docs / future
        Ok(out)
    }
}
