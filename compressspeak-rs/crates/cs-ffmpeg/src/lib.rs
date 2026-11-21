//! FFmpeg command builder and runner (port of ffmpeg.py).

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use thiserror::Error;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn configure_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd;
}

#[derive(Debug, Error)]
pub enum FfmpegError {
    #[error("FFmpeg introuvable (PATH ou vendor)")]
    NotFound,
    #[error("échec FFmpeg: {0}")]
    Failed(String),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionParams {
    pub afftdn_nf: i32,
    pub comp_threshold_db: f32,
    pub comp_ratio: f32,
    pub comp_attack_ms: f32,
    pub comp_release_ms: f32,
    pub opus_bitrate: String,
    pub opus_preview_bitrate: String,
    pub skip_afftdn: bool,
}

impl Default for ConversionParams {
    fn default() -> Self {
        Self {
            afftdn_nf: -25,
            comp_threshold_db: -21.0,
            comp_ratio: 2.5,
            comp_attack_ms: 200.0,
            comp_release_ms: 1000.0,
            opus_bitrate: "20k".into(),
            opus_preview_bitrate: "64k".into(),
            skip_afftdn: false,
        }
    }
}

impl ConversionParams {
    pub fn preset_audiobook() -> Self {
        Self {
            afftdn_nf: -30,
            comp_threshold_db: -18.0,
            comp_ratio: 2.0,
            comp_attack_ms: 200.0,
            comp_release_ms: 1000.0,
            opus_bitrate: "32k".into(),
            ..Default::default()
        }
    }

    pub fn preset_podcast() -> Self {
        Self::default()
    }

    pub fn preset_voice_chat() -> Self {
        Self {
            afftdn_nf: -22,
            comp_threshold_db: -24.0,
            comp_ratio: 3.0,
            opus_bitrate: "16k".into(),
            ..Default::default()
        }
    }

    pub fn preset_voice_note() -> Self {
        Self {
            afftdn_nf: -20,
            comp_threshold_db: -24.0,
            comp_ratio: 4.0,
            opus_bitrate: "12k".into(),
            ..Default::default()
        }
    }
}

/// Resolve ffmpeg binary: vendor/<os>/ffmpeg[.exe] under data dir, then PATH.
pub fn resolve_ffmpeg(data_dir: Option<&Path>) -> Result<PathBuf, FfmpegError> {
    if let Some(dir) = data_dir {
        let vendor = dir.join("ffmpeg").join(platform_vendor_dir()).join(ffmpeg_exe_name());
        if vendor.is_file() {
            return Ok(vendor);
        }
        // Also accept flat layout: data_dir/ffmpeg.exe
        let flat = dir.join(ffmpeg_exe_name());
        if flat.is_file() {
            return Ok(flat);
        }
    }
    // Relative vendor next to executable / cwd
    for base in [PathBuf::from("vendor/ffmpeg"), PathBuf::from("../vendor/ffmpeg")] {
        let p = base.join(platform_vendor_dir()).join(ffmpeg_exe_name());
        if p.is_file() {
            return Ok(p);
        }
    }
    which::which("ffmpeg").map_err(|_| FfmpegError::NotFound)
}

fn platform_vendor_dir() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn ffmpeg_exe_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

pub fn ffmpeg_available(data_dir: Option<&Path>) -> bool {
    resolve_ffmpeg(data_dir).is_ok()
}

fn expand_zip(zip: &Path, dest: &Path) -> Result<(), FfmpegError> {
    std::fs::create_dir_all(dest)?;
    #[cfg(windows)]
    {
        let status = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!(
                    "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                    zip.display(),
                    dest.display()
                ),
            ])
            .status()
            .map_err(|e| FfmpegError::Failed(e.to_string()))?;
        if !status.success() {
            return Err(FfmpegError::Failed(
                "Expand-Archive PowerShell a échoué".into(),
            ));
        }
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let status = Command::new("unzip")
            .args(["-o", &zip.to_string_lossy(), "-d", &dest.to_string_lossy()])
            .status()
            .map_err(|e| FfmpegError::Failed(e.to_string()))?;
        if !status.success() {
            return Err(FfmpegError::Failed("unzip a échoué".into()));
        }
        Ok(())
    }
}

fn find_named_file(root: &Path, name: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let p = ent.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().and_then(|n| n.to_str()) == Some(name) {
                return Some(p);
            }
        }
    }
    None
}

/// Télécharge FFmpeg essentials dans `{data_dir}/ffmpeg/<os>/`.
pub fn install_ffmpeg(data_dir: &Path) -> Result<PathBuf, FfmpegError> {
    let dest_dir = data_dir.join("ffmpeg").join(platform_vendor_dir());
    std::fs::create_dir_all(&dest_dir)?;
    let dest_exe = dest_dir.join(ffmpeg_exe_name());
    if dest_exe.is_file() {
        return Ok(dest_exe);
    }

    #[cfg(windows)]
    let url = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
    #[cfg(target_os = "macos")]
    let url = "https://evermeet.cx/ffmpeg/getrelease/zip";
    #[cfg(all(unix, not(target_os = "macos")))]
    let url = "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz";

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = url;
        return Err(FfmpegError::Failed(
            "Installation auto FFmpeg Linux non supportée — installez via le gestionnaire de paquets"
                .into(),
        ));
    }

    tracing::info!("Téléchargement FFmpeg…");
    let tmp = tempfile::tempdir()?;
    let zip_path = tmp.path().join("ffmpeg.zip");
    let resp = ureq::get(url)
        .call()
        .map_err(|e| FfmpegError::Failed(e.to_string()))?;
    let bytes = resp
        .into_body()
        .read_to_vec()
        .map_err(|e| FfmpegError::Failed(e.to_string()))?;
    if bytes.len() < 1_000_000 {
        return Err(FfmpegError::Failed(format!(
            "archive FFmpeg trop petite ({} octets)",
            bytes.len()
        )));
    }
    std::fs::write(&zip_path, &bytes)?;

    let extract = tmp.path().join("extract");
    expand_zip(&zip_path, &extract)?;
    let found = find_named_file(&extract, ffmpeg_exe_name()).ok_or_else(|| {
        FfmpegError::Failed(format!("{} introuvable dans l'archive", ffmpeg_exe_name()))
    })?;
    std::fs::copy(&found, &dest_exe)?;
    if let Some(probe) = find_named_file(
        &extract,
        if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        },
    ) {
        let _ = std::fs::copy(probe, dest_dir.join(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        }));
    }
    tracing::info!("FFmpeg installé: {}", dest_exe.display());
    Ok(dest_exe)
}

pub fn build_af_filter(gain_db: i32, params: &ConversionParams) -> String {
    let tail = format!(
        "volume={}dB,acompressor=threshold={}dB:ratio={}:attack={}:release={}",
        gain_db,
        params.comp_threshold_db,
        params.comp_ratio,
        params.comp_attack_ms,
        params.comp_release_ms
    );
    if params.skip_afftdn {
        tail
    } else {
        format!("afftdn=nf={},{}", params.afftdn_nf, tail)
    }
}

pub fn build_convert_command(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    gain_db: i32,
    params: &ConversionParams,
) -> Vec<String> {
    vec![
        ffmpeg.to_string_lossy().into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into(),
        "-vn".into(),
        "-af".into(),
        build_af_filter(gain_db, params),
        "-c:a".into(),
        "libopus".into(),
        "-b:a".into(),
        params.opus_bitrate.clone(),
        "-vbr".into(),
        "on".into(),
        "-application".into(),
        "voip".into(),
        output.to_string_lossy().into(),
    ]
}

pub fn build_preview_command(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    start_sec: f64,
    duration_sec: f64,
    gain_db: i32,
    params: &ConversionParams,
) -> Vec<String> {
    vec![
        ffmpeg.to_string_lossy().into(),
        "-y".into(),
        "-ss".into(),
        format!("{start_sec:.3}"),
        "-i".into(),
        input.to_string_lossy().into(),
        "-t".into(),
        format!("{duration_sec:.3}"),
        "-vn".into(),
        "-af".into(),
        build_af_filter(gain_db, params),
        "-c:a".into(),
        "libopus".into(),
        "-b:a".into(),
        params.opus_preview_bitrate.clone(),
        "-vbr".into(),
        "on".into(),
        "-application".into(),
        "voip".into(),
        output.to_string_lossy().into(),
    ]
}

pub fn build_m4a_to_wav_command(ffmpeg: &Path, input: &Path, output_wav: &Path) -> Vec<String> {
    vec![
        ffmpeg.to_string_lossy().into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into(),
        "-vn".into(),
        "-acodec".into(),
        "pcm_s16le".into(),
        output_wav.to_string_lossy().into(),
    ]
}

pub fn build_mono_wav_command(
    ffmpeg: &Path,
    input: &Path,
    output_wav: &Path,
    sample_rate: u32,
) -> Vec<String> {
    vec![
        ffmpeg.to_string_lossy().into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into(),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        sample_rate.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        output_wav.to_string_lossy().into(),
    ]
}

/// Decode / resample to stereo PCM WAV (UVR MDX-Net expects 44.1 kHz stereo).
pub fn build_stereo_wav_command(
    ffmpeg: &Path,
    input: &Path,
    output_wav: &Path,
    sample_rate: u32,
) -> Vec<String> {
    vec![
        ffmpeg.to_string_lossy().into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into(),
        "-vn".into(),
        "-ac".into(),
        "2".into(),
        "-ar".into(),
        sample_rate.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        output_wav.to_string_lossy().into(),
    ]
}

pub fn parse_progress_from_output(text: &str, duration_sec: f64) -> Option<u8> {
    if duration_sec <= 0.0 {
        return None;
    }
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"time=(\d+):(\d+):(\d+\.?\d*)").unwrap());
    let caps = re.captures(text)?;
    let hours: f64 = caps[1].parse().ok()?;
    let minutes: f64 = caps[2].parse().ok()?;
    let seconds: f64 = caps[3].parse().ok()?;
    let current = hours * 3600.0 + minutes * 60.0 + seconds;
    Some(((current / duration_sec) * 100.0).clamp(0.0, 100.0) as u8)
}

pub fn probe_duration_sec(ffmpeg: &Path, input: &Path) -> Option<f64> {
    let ffprobe = ffmpeg
        .parent()
        .map(|p| p.join(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" }))
        .filter(|p| p.is_file())
        .or_else(|| which::which("ffprobe").ok());

    if let Some(probe) = ffprobe {
        let mut cmd = Command::new(probe);
        cmd.args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            &input.to_string_lossy(),
        ]);
        configure_no_window(&mut cmd);
        let out = cmd.output().ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        return s.parse().ok();
    }

    // Fallback: parse ffmpeg -i stderr
    let mut ff = Command::new(ffmpeg);
    configure_no_window(&mut ff);
    let out = ff
        .args(["-i", &input.to_string_lossy()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    let err = String::from_utf8_lossy(&out.stderr);
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"Duration: (\d+):(\d+):(\d+\.?\d*)").unwrap());
    let caps = re.captures(&err)?;
    let h: f64 = caps[1].parse().ok()?;
    let m: f64 = caps[2].parse().ok()?;
    let s: f64 = caps[3].parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

pub fn run_command(args: &[String]) -> Result<(), FfmpegError> {
    if args.is_empty() {
        return Err(FfmpegError::NotFound);
    }
    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    configure_no_window(&mut cmd);
    let output = cmd.output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(FfmpegError::Failed(err.trim().to_string()));
    }
    Ok(())
}

/// Run with progress callback on stderr lines.
pub fn run_command_with_progress<F>(
    args: &[String],
    duration_sec: f64,
    mut on_progress: F,
) -> Result<(), FfmpegError>
where
    F: FnMut(u8),
{
    use std::io::{BufRead, BufReader};
    if args.is_empty() {
        return Err(FfmpegError::NotFound);
    }
    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    configure_no_window(&mut cmd);
    let mut child = cmd.spawn()?;
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        for line in reader.lines().flatten() {
            if let Some(p) = parse_progress_from_output(&line, duration_sec) {
                on_progress(p);
            }
        }
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(FfmpegError::Failed(format!("exit {status}")));
    }
    on_progress(100);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn af_filter_default() {
        let p = ConversionParams::default();
        let f = build_af_filter(18, &p);
        assert!(f.contains("afftdn=nf=-25"));
        assert!(f.contains("volume=18dB"));
    }
}
