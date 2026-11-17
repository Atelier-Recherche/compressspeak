//! Optional UVR5 / audio-separator CLI integration (not bundled).

use cs_ffmpeg::{build_m4a_to_wav_command, resolve_ffmpeg, run_command};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Uvr5Error {
    #[error("audio-separator / UVR5 CLI introuvable")]
    NotFound,
    #[error("UVR5: {0}")]
    Failed(String),
    #[error("FFmpeg: {0}")]
    Ffmpeg(String),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Uvr5Params {
    pub path: String,
    pub model: String,
    pub stem: String,
    pub chunk_sec: u32,
    pub mdx_segment_size: u32,
    pub mdxc_overlap: u32,
    pub mdx_batch_size: u32,
}

impl Default for Uvr5Params {
    fn default() -> Self {
        Self {
            path: String::new(),
            model: String::new(),
            stem: "Vocals".into(),
            chunk_sec: 1200,
            mdx_segment_size: 2048,
            mdxc_overlap: 8,
            mdx_batch_size: 1,
        }
    }
}

pub const AVAILABLE_STEMS: &[&str] = &[
    "Vocals",
    "Instrumental",
    "Drums",
    "Bass",
    "Guitar",
    "Piano",
    "Other",
];

pub fn resolve_command(uvr_path: &str) -> Option<Vec<String>> {
    let path = uvr_path.trim();
    if path.is_empty() || path == "audio-separator" {
        return which::which("audio-separator")
            .ok()
            .map(|p| vec![p.to_string_lossy().into_owned()]);
    }
    let p = PathBuf::from(path);
    if p.is_file() {
        if p.extension().and_then(|e| e.to_str()) == Some("py") {
            let python = which::which("python").ok()?.to_string_lossy().into_owned();
            return Some(vec![python, p.to_string_lossy().into_owned()]);
        }
        return Some(vec![p.to_string_lossy().into_owned()]);
    }
    if p.is_dir() {
        #[cfg(windows)]
        let candidates = [
            p.join("venv/Scripts/audio-separator.exe"),
            p.join(".venv/Scripts/audio-separator.exe"),
            p.join("Scripts/audio-separator.exe"),
            p.join("audio-separator.exe"),
        ];
        #[cfg(not(windows))]
        let candidates = [
            p.join("venv/bin/audio-separator"),
            p.join(".venv/bin/audio-separator"),
            p.join("bin/audio-separator"),
        ];
        for c in candidates {
            if c.is_file() {
                return Some(vec![c.to_string_lossy().into_owned()]);
            }
        }
        #[cfg(windows)]
        {
            if (p.join("UVR.exe").exists()) || (p.join("UVR_Launcher.exe").exists()) {
                return None; // GUI only
            }
        }
    }
    Some(vec![path.to_string()])
}

/// Tente `python -m pip install audio-separator` (ou `py -m pip` sous Windows).
pub fn install_via_pip() -> Result<String, Uvr5Error> {
    let candidates: &[&[&str]] = if cfg!(windows) {
        &[
            &["py", "-3", "-m", "pip", "install", "--user", "audio-separator"],
            &["python", "-m", "pip", "install", "--user", "audio-separator"],
            &["python3", "-m", "pip", "install", "--user", "audio-separator"],
        ]
    } else {
        &[
            &["python3", "-m", "pip", "install", "--user", "audio-separator"],
            &["python", "-m", "pip", "install", "--user", "audio-separator"],
        ]
    };
    let mut last_err = String::from("Python / pip introuvable");
    for args in candidates {
        let (prog, rest) = args.split_first().unwrap();
        let output = match Command::new(prog).args(rest).output() {
            Ok(o) => o,
            Err(e) => {
                last_err = e.to_string();
                continue;
            }
        };
        if output.status.success() {
            return Ok(format!(
                "audio-separator installé via {}",
                args.join(" ")
            ));
        }
        last_err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if last_err.is_empty() {
            last_err = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }
    Err(Uvr5Error::Failed(last_err))
}

pub fn is_available(params: &Uvr5Params) -> bool {
    resolve_command(&params.path).is_some()
}

/// Run vocal isolation; returns path to primary stem file (flac/wav).
pub fn separate_vocals(
    input: &Path,
    output_dir: &Path,
    params: &Uvr5Params,
    data_dir: Option<&Path>,
) -> Result<PathBuf, Uvr5Error> {
    let cmd_base = resolve_command(&params.path).ok_or(Uvr5Error::NotFound)?;
    std::fs::create_dir_all(output_dir)?;

    let ffmpeg = resolve_ffmpeg(data_dir).map_err(|e| Uvr5Error::Ffmpeg(e.to_string()))?;
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let work_input = if matches!(ext.as_str(), "m4a" | "mp4" | "aac") {
        let wav = output_dir.join(format!(
            "{}_prep.wav",
            input.file_stem().unwrap_or_default().to_string_lossy()
        ));
        let c = build_m4a_to_wav_command(&ffmpeg, input, &wav);
        run_command(&c).map_err(|e| Uvr5Error::Ffmpeg(e.to_string()))?;
        wav
    } else {
        input.to_path_buf()
    };

    let mut args = cmd_base;
    args.push(work_input.to_string_lossy().into_owned());
    args.push("--output_dir".into());
    args.push(output_dir.to_string_lossy().into_owned());
    if !params.model.is_empty() {
        args.push("--model_filename".into());
        args.push(params.model.clone());
    }
    args.push("--single_stem".into());
    args.push(params.stem.clone());

    tracing::info!("UVR5: {}", work_input.display());
    let mut child = Command::new(&args[0]);
    child.args(&args[1..]).stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = child.output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(Uvr5Error::Failed(err.trim().to_string()));
    }

    // Find newest stem file in output_dir
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(output_dir)? {
        let entry = entry?;
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "flac" | "wav" | "mp3") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !name.contains(&params.stem.to_ascii_lowercase()) && params.stem != "Vocals" {
            // still accept if only one audio file
        }
        let modified = entry.metadata()?.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        match &best {
            None => best = Some((modified, path)),
            Some((t, _)) if modified > *t => best = Some((modified, path)),
            _ => {}
        }
    }
    best.map(|(_, p)| p)
        .ok_or_else(|| Uvr5Error::Failed("aucun stem trouvé en sortie".into()))
}
