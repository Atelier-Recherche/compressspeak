//! ONNX Runtime session for Inworld Voice Isolator offline model.

use crate::stft::{enhance_chunks, StftKit, BINS};
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
        .with_name("compressspeak-inworld")
        .build()
        .map_err(|e| e.to_string())?;
    let _ = ENV.set(Arc::new(env));
    ENV.get().ok_or_else(|| "ORT env".into())
}

pub struct InworldSession {
    session: Session,
    in_name: String,
    out_name: String,
    stft: StftKit,
}

impl InworldSession {
    pub fn load(path: &Path) -> Result<Self, String> {
        let env = environment()?;
        let session = Session::builder(env)
            .map_err(|e| e.to_string())?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?
            .with_intra_threads(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4))
            .map_err(|e| e.to_string())?
            .commit_from_file(path)
            .map_err(|e| format!("ONNX {}: {e}", path.display()))?;

        let inputs = session.inputs();
        let outputs = session.outputs();
        if inputs.is_empty() || outputs.is_empty() {
            return Err("modèle ONNX: entrée/sortie manquante".into());
        }
        let in_name = inputs[0].name().to_string();
        let out_name = outputs[0].name().to_string();

        Ok(Self {
            session,
            in_name,
            out_name,
            stft: StftKit::new(),
        })
    }

    fn run_spec(&mut self, spec: &[f32], frames: usize) -> Result<Vec<f32>, String> {
        let expected = frames * BINS * 2;
        if spec.len() != expected {
            return Err(format!("spec len {} != expected {expected}", spec.len()));
        }
        let shape = [1usize, frames, BINS, 2];
        let tensor = TensorRef::from_array_view((shape, spec)).map_err(|e| e.to_string())?;
        let outputs = self
            .session
            .run(ort::inputs![self.in_name.as_str() => tensor])
            .map_err(|e| e.to_string())?;
        let (_, slice) = outputs
            .get(self.out_name.as_str())
            .ok_or_else(|| format!("sortie '{}' absente", self.out_name))?
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        if slice.len() != expected {
            return Err(format!(
                "correction len {} != expected {expected}",
                slice.len()
            ));
        }
        Ok(slice.to_vec())
    }

    fn isolate_window(&mut self, x: &[f32]) -> Result<Vec<f32>, String> {
        let (spec, frames) = self.stft.stft(x);
        let correction = self.run_spec(&spec, frames)?;
        let delta = self.stft.istft(&correction, frames, x.len());
        let mut y = Vec::with_capacity(x.len());
        for i in 0..x.len() {
            y.push(x[i] + delta[i]);
        }
        Ok(y)
    }

    /// Isolate voice from mono f32 @ 48 kHz; same length as input.
    pub fn isolate(&mut self, audio: &[f32]) -> Result<Vec<f32>, String> {
        if audio.is_empty() {
            return Ok(Vec::new());
        }
        enhance_chunks(audio, |chunk| self.isolate_window(chunk))
    }
}
