//! Transcription via whisper.cpp CLI or Vibe (`vibe-server`),
//! réutilisant les modèles OpenWhispr / Vibe déjà installés.

use cs_ffmpeg::{build_mono_wav_command, resolve_ffmpeg, run_command};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Hide console flash when spawning CLI tools on Windows.
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
pub enum WhisperError {
    #[error("Whisper / Vibe introuvable (installez OpenWhispr, Vibe, ou whisper-cli)")]
    CliNotFound,
    #[error("modèle: {0}")]
    Model(String),
    #[error("transcription: {0}")]
    Failed(String),
    #[error("FFmpeg: {0}")]
    Ffmpeg(String),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("téléchargement: {0}")]
    Download(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptFormat {
    Txt,
    Srt,
    Vtt,
    Json,
}

impl TranscriptFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Txt => "txt",
            Self::Srt => "srt",
            Self::Vtt => "vtt",
            Self::Json => "json",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "srt" => Self::Srt,
            "vtt" => Self::Vtt,
            "json" => Self::Json,
            _ => Self::Txt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    WhisperCli,
    VibeServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineSource {
    Custom,
    Vendor,
    Path,
    Vibe,
}

#[derive(Debug, Clone)]
pub struct ResolvedEngine {
    pub path: PathBuf,
    pub kind: EngineKind,
    pub source: EngineSource,
    /// Working directory required for vibe-server (DLL lookup).
    pub work_dir: Option<PathBuf>,
}

impl ResolvedEngine {
    pub fn label(&self) -> &'static str {
        match (self.kind, self.source) {
            (EngineKind::WhisperCli, EngineSource::Custom) => "whisper-cli (personnalisé)",
            (EngineKind::WhisperCli, EngineSource::Vendor) => "whisper-cli (CompressSpeak)",
            (EngineKind::WhisperCli, EngineSource::Path) => "whisper-cli (PATH)",
            (EngineKind::WhisperCli, EngineSource::Vibe) => "whisper-cli",
            (EngineKind::VibeServer, _) => "Vibe (vibe-server)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSource {
    CompressSpeak,
    OpenWhispr,
    Vibe,
    Downloaded,
}

#[derive(Debug, Clone)]
pub struct DiscoveredModel {
    pub name: String,
    pub path: PathBuf,
    pub source: ModelSource,
}

impl DiscoveredModel {
    pub fn source_label(&self) -> &'static str {
        match self.source {
            ModelSource::CompressSpeak => "CompressSpeak",
            ModelSource::OpenWhispr => "OpenWhispr",
            ModelSource::Vibe => "Vibe",
            ModelSource::Downloaded => "téléchargé",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperParams {
    pub model: String,
    pub language: String,
    pub format: TranscriptFormat,
    /// Optional path to whisper-cli / main.exe / vibe-server.exe
    pub cli_path: String,
}

impl Default for WhisperParams {
    fn default() -> Self {
        Self {
            model: "small".into(),
            language: "fr".into(),
            format: TranscriptFormat::Txt,
            cli_path: String::new(),
        }
    }
}

pub const MODEL_NAMES: &[&str] = &["tiny", "base", "small", "medium", "large-v3", "large-v3-turbo"];

pub fn data_root() -> PathBuf {
    directories::ProjectDirs::from("com", "CompressSpeak", "Compress Speak")
        .map(|d| d.data_local_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn models_dir() -> PathBuf {
    data_root().join("models").join("whisper")
}

fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            directories::BaseDirs::new()
                .map(|b| b.data_local_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        })
}

fn user_home() -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// OpenWhispr install dir (Windows): `%LOCALAPPDATA%\Programs\OpenWhispr`
pub fn openwhispr_install_dir() -> Option<PathBuf> {
    let p = local_app_data().join("Programs").join("OpenWhispr");
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// OpenWhispr whisper models: `~/.cache/openwhispr/whisper-models`
pub fn openwhispr_models_dir() -> Option<PathBuf> {
    let candidates = [
        user_home().join(".cache").join("openwhispr").join("whisper-models"),
        PathBuf::from(std::env::var_os("ProgramData").unwrap_or_default())
            .join("OpenWhispr")
            .join("cache")
            .join("whisper-models"),
        local_app_data()
            .join("OpenWhispr")
            .join("cache")
            .join("whisper-models"),
    ];
    candidates.into_iter().find(|p| p.is_dir())
}

/// Vibe desktop install: `%LOCALAPPDATA%\vibe`
pub fn vibe_install_dir() -> Option<PathBuf> {
    let p = local_app_data().join("vibe");
    if p.join("vibe-server.exe").is_file() || p.join("vibe-server").is_file() || p.join("vibe.exe").is_file()
    {
        Some(p)
    } else {
        None
    }
}

/// Vibe ggml models: `%LOCALAPPDATA%\github.com.thewh1teagle.vibe`
pub fn vibe_models_dir() -> Option<PathBuf> {
    let candidates = [
        local_app_data().join("github.com.thewh1teagle.vibe"),
        user_home()
            .join("Library")
            .join("Application Support")
            .join("github.com.thewh1teagle.vibe"),
        user_home()
            .join(".local")
            .join("share")
            .join("github.com.thewh1teagle.vibe"),
    ];
    candidates.into_iter().find(|p| p.is_dir())
}

fn model_display_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model");
    stem.strip_prefix("ggml-").unwrap_or(stem).to_string()
}

fn scan_ggml_models(dir: &Path, source: ModelSource, out: &mut Vec<DiscoveredModel>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if !p.is_file() {
            continue;
        }
        let name = match p.file_name().and_then(|n| n.to_str()) {
            Some(n) if n.starts_with("ggml-") && n.ends_with(".bin") && !n.contains("silero") => n,
            _ => continue,
        };
        // Skip tiny VAD / non-whisper blobs
        if let Ok(meta) = ent.metadata() {
            if meta.len() < 1_000_000 {
                continue;
            }
        }
        let display = model_display_name(&p);
        if out.iter().any(|m| m.name == display || m.path == p) {
            continue;
        }
        let _ = name;
        out.push(DiscoveredModel {
            name: display,
            path: p,
            source,
        });
    }
}

/// Models: CompressSpeak → OpenWhispr → Vibe (priority for listing; first match wins per name).
pub fn list_discovered_models() -> Vec<DiscoveredModel> {
    let mut out = Vec::new();
    scan_ggml_models(&models_dir(), ModelSource::CompressSpeak, &mut out);
    if let Some(dir) = openwhispr_models_dir() {
        scan_ggml_models(&dir, ModelSource::OpenWhispr, &mut out);
    }
    if let Some(dir) = vibe_models_dir() {
        scan_ggml_models(&dir, ModelSource::Vibe, &mut out);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn ggml_filename(model: &str) -> String {
    if model.starts_with("ggml-") && model.ends_with(".bin") {
        model.to_string()
    } else {
        format!("ggml-{model}.bin")
    }
}

fn model_url(model: &str) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        ggml_filename(model)
    )
}

/// Resolve a model name or path. Prefers OpenWhispr, then Vibe, then local, else download.
pub fn resolve_model(model: &str) -> Result<(PathBuf, ModelSource), WhisperError> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return Err(WhisperError::Model("nom de modèle vide".into()));
    }
    let as_path = PathBuf::from(trimmed);
    if as_path.is_file() {
        return Ok((as_path, ModelSource::CompressSpeak));
    }

    let fname = ggml_filename(trimmed);
    let short = model_display_name(Path::new(&fname));

    // Exact / fuzzy match in discovered models (OpenWhispr first via scan order in list — re-scan by priority)
    let search_dirs: [(Option<PathBuf>, ModelSource); 3] = [
        (openwhispr_models_dir(), ModelSource::OpenWhispr),
        (vibe_models_dir(), ModelSource::Vibe),
        (Some(models_dir()), ModelSource::CompressSpeak),
    ];
    for (dir, source) in search_dirs {
        let Some(dir) = dir else { continue };
        let exact = dir.join(&fname);
        if exact.is_file() {
            return Ok((exact, source));
        }
        // Fuzzy: ggml-large-v3-q5_0.bin matches "large-v3" / "large"
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for ent in rd.flatten() {
                let p = ent.path();
                let n = p.file_name().and_then(|x| x.to_str()).unwrap_or("");
                if !n.starts_with("ggml-") || !n.ends_with(".bin") || n.contains("silero") {
                    continue;
                }
                let display = model_display_name(&p);
                if display == short
                    || display.starts_with(&format!("{short}-"))
                    || short.starts_with(&display)
                {
                    if let Ok(meta) = ent.metadata() {
                        if meta.len() >= 1_000_000 {
                            return Ok((p, source));
                        }
                    }
                }
            }
        }
    }

    // Download into CompressSpeak data dir
    let path = ensure_model_download(trimmed)?;
    Ok((path, ModelSource::Downloaded))
}

pub fn ensure_model(model: &str) -> Result<PathBuf, WhisperError> {
    resolve_model(model).map(|(p, _)| p)
}

fn ensure_model_download(model: &str) -> Result<PathBuf, WhisperError> {
    let dir = models_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(ggml_filename(model));
    if path.is_file() {
        return Ok(path);
    }
    tracing::info!("Téléchargement Whisper {model}…");
    let url = model_url(model);
    let resp = ureq::get(&url)
        .call()
        .map_err(|e| WhisperError::Download(e.to_string()))?;
    let bytes = resp
        .into_body()
        .read_to_vec()
        .map_err(|e| WhisperError::Download(e.to_string()))?;
    if bytes.len() < 1_000_000 {
        return Err(WhisperError::Download(format!(
            "modèle trop petit ({} octets) — vérifiez le réseau",
            bytes.len()
        )));
    }
    std::fs::write(&path, bytes)?;
    Ok(path)
}

fn classify_cli(path: &Path) -> EngineKind {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.contains("vibe-server") || name == "vibe.exe" || name == "vibe" {
        EngineKind::VibeServer
    } else {
        EngineKind::WhisperCli
    }
}

/// Refuse Windows CPL / non-PE (ex. `main.cpl` via `which("main")` → erreur 193).
fn is_plausible_cli(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // Never treat shell/control-panel modules as whisper
    if name == "main.cpl" || name.ends_with(".cpl") || name.ends_with(".scr") {
        return false;
    }
    #[cfg(windows)]
    {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "exe" | "bat" | "cmd" | "com" | "") {
            return false;
        }
        // PE MZ header
        if let Ok(mut f) = std::fs::File::open(path) {
            use std::io::Read;
            let mut magic = [0u8; 2];
            if f.read_exact(&mut magic).is_ok() && magic == *b"MZ" {
                return true;
            }
            // .bat/.cmd without MZ
            return matches!(ext.as_str(), "bat" | "cmd");
        }
        return false;
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
}

fn vibe_server_path(install: &Path) -> Option<PathBuf> {
    for name in ["vibe-server.exe", "vibe-server"] {
        let p = install.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

pub fn resolve_whisper_engine(custom: &str) -> Option<ResolvedEngine> {
    if !custom.trim().is_empty() {
        let p = PathBuf::from(custom.trim());
        if p.is_file() && is_plausible_cli(&p) {
            // vibe.exe (GUI) → prefer sibling vibe-server for CLI transcription
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if name == "vibe.exe" || name == "vibe" {
                if let Some(parent) = p.parent() {
                    if let Some(server) = vibe_server_path(parent) {
                        return Some(ResolvedEngine {
                            path: server,
                            kind: EngineKind::VibeServer,
                            source: EngineSource::Custom,
                            work_dir: Some(parent.to_path_buf()),
                        });
                    }
                }
            }
            let kind = classify_cli(&p);
            let work_dir = if kind == EngineKind::VibeServer {
                p.parent().map(|d| d.to_path_buf())
            } else {
                None
            };
            return Some(ResolvedEngine {
                path: p,
                kind,
                source: EngineSource::Custom,
                work_dir,
            });
        }
    }

    let vendor = data_root().join("whisper").join(if cfg!(windows) {
        "whisper-cli.exe"
    } else {
        "whisper-cli"
    });
    if vendor.is_file() && is_plausible_cli(&vendor) {
        return Some(ResolvedEngine {
            path: vendor,
            kind: EngineKind::WhisperCli,
            source: EngineSource::Vendor,
            work_dir: None,
        });
    }

    // Explicit whisper-cli only — never `main` (Windows → main.cpl, erreur 193)
    for name in ["whisper-cli", "whisper-cli.exe"] {
        if let Ok(p) = which::which(name) {
            if is_plausible_cli(&p) {
                return Some(ResolvedEngine {
                    path: p,
                    kind: EngineKind::WhisperCli,
                    source: EngineSource::Path,
                    work_dir: None,
                });
            }
        }
    }

    // Vibe desktop before generic `whisper` on PATH (often a Python shim)
    if let Some(install) = vibe_install_dir() {
        if let Some(server) = vibe_server_path(&install) {
            if is_plausible_cli(&server) {
                return Some(ResolvedEngine {
                    path: server,
                    kind: EngineKind::VibeServer,
                    source: EngineSource::Vibe,
                    work_dir: Some(install),
                });
            }
        }
    }

    for name in ["whisper", "whisper.exe"] {
        if let Ok(p) = which::which(name) {
            if is_plausible_cli(&p) {
                return Some(ResolvedEngine {
                    path: p,
                    kind: EngineKind::WhisperCli,
                    source: EngineSource::Path,
                    work_dir: None,
                });
            }
        }
    }

    None
}

/// Back-compat alias.
pub fn resolve_whisper_cli(custom: &str) -> Option<PathBuf> {
    resolve_whisper_engine(custom).map(|e| e.path)
}

pub fn is_available(custom_cli: &str) -> bool {
    resolve_whisper_engine(custom_cli).is_some()
}

fn expand_zip(zip: &Path, dest: &Path) -> Result<(), WhisperError> {
    std::fs::create_dir_all(dest)?;
    #[cfg(windows)]
    {
        let status = std::process::Command::new("powershell")
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
            .status()?;
        if !status.success() {
            return Err(WhisperError::Download(
                "Expand-Archive PowerShell a échoué".into(),
            ));
        }
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let status = std::process::Command::new("unzip")
            .args(["-o", &zip.to_string_lossy(), "-d", &dest.to_string_lossy()])
            .status()?;
        if !status.success() {
            return Err(WhisperError::Download("unzip a échoué".into()));
        }
        Ok(())
    }
}

fn find_file_named(root: &Path, names: &[&str]) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let p = ent.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let n = p.file_name().and_then(|x| x.to_str()).unwrap_or("");
            if names.iter().any(|want| n.eq_ignore_ascii_case(want)) {
                return Some(p);
            }
        }
    }
    None
}

fn latest_whisper_bin_url() -> Result<String, WhisperError> {
    // Prefer a release that still ships prebuilt Windows zips (nightly tags like b5454).
    let api = "https://api.github.com/repos/ggml-org/whisper.cpp/releases?per_page=20";
    let resp = ureq::get(api)
        .header("User-Agent", "CompressSpeak")
        .call()
        .map_err(|e| WhisperError::Download(e.to_string()))?;
    let body = resp
        .into_body()
        .read_to_string()
        .map_err(|e| WhisperError::Download(e.to_string()))?;
    let releases: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| WhisperError::Download(e.to_string()))?;
    let want = if cfg!(windows) {
        "whisper-bin-x64.zip"
    } else if cfg!(target_os = "macos") {
        // no official mac zip in all releases — fall through
        "whisper-bin-x64.zip"
    } else {
        "whisper-bin-ubuntu-x64.tar.gz"
    };
    if let Some(arr) = releases.as_array() {
        for rel in arr {
            if let Some(assets) = rel.get("assets").and_then(|a| a.as_array()) {
                for asset in assets {
                    let name = asset.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    if name == want {
                        if let Some(url) = asset
                            .get("browser_download_url")
                            .and_then(|u| u.as_str())
                        {
                            return Ok(url.to_string());
                        }
                    }
                }
            }
        }
    }
    Err(WhisperError::Download(format!(
        "aucun asset {want} trouvé sur les releases whisper.cpp"
    )))
}

/// Télécharge whisper-cli dans `{data}/whisper/`.
pub fn install_whisper_cli() -> Result<PathBuf, WhisperError> {
    let dest_dir = data_root().join("whisper");
    std::fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(if cfg!(windows) {
        "whisper-cli.exe"
    } else {
        "whisper-cli"
    });
    if dest.is_file() {
        return Ok(dest);
    }

    #[cfg(target_os = "macos")]
    {
        return Err(WhisperError::Download(
            "Install auto whisper-cli macOS non supportée — utilisez brew install whisper-cpp ou Vibe"
                .into(),
        ));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return Err(WhisperError::Download(
            "Install auto whisper-cli Linux non supportée pour les tar.gz — utilisez le paquet distro ou Vibe"
                .into(),
        ));
    }

    let url = latest_whisper_bin_url()?;
    tracing::info!("Téléchargement whisper-cli: {url}");
    let tmp = tempfile::tempdir()?;
    let zip_path = tmp.path().join("whisper.zip");
    let resp = ureq::get(&url)
        .header("User-Agent", "CompressSpeak")
        .call()
        .map_err(|e| WhisperError::Download(e.to_string()))?;
    let bytes = resp
        .into_body()
        .read_to_vec()
        .map_err(|e| WhisperError::Download(e.to_string()))?;
    if bytes.len() < 100_000 {
        return Err(WhisperError::Download(format!(
            "archive trop petite ({} octets)",
            bytes.len()
        )));
    }
    std::fs::write(&zip_path, &bytes)?;
    let extract = tmp.path().join("extract");
    expand_zip(&zip_path, &extract)?;
    let found = find_file_named(
        &extract,
        &["whisper-cli.exe", "whisper-cli", "main.exe", "main"],
    )
    .ok_or_else(|| WhisperError::Download("whisper-cli introuvable dans l'archive".into()))?;
    std::fs::copy(&found, &dest)?;
    // Copy sibling DLLs next to the CLI (Windows CUDA/CPU builds)
    if let Some(parent) = found.parent() {
        if let Ok(rd) = std::fs::read_dir(parent) {
            for ent in rd.flatten() {
                let p = ent.path();
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if p.is_file()
                    && (name.ends_with(".dll") || name.ends_with(".so") || name.ends_with(".dylib"))
                {
                    let _ = std::fs::copy(&p, dest_dir.join(name));
                }
            }
        }
    }
    tracing::info!("whisper-cli installé: {}", dest.display());
    Ok(dest)
}

/// High-level detection status for UI / logs.
pub fn detection_status(custom_cli: &str) -> String {
    let engine = resolve_whisper_engine(custom_cli);
    let ow = openwhispr_install_dir().is_some() || openwhispr_models_dir().is_some();
    let vibe = vibe_install_dir().is_some() || vibe_models_dir().is_some();
    let models = list_discovered_models();
    let ow_models = models
        .iter()
        .filter(|m| m.source == ModelSource::OpenWhispr)
        .count();
    let vibe_models = models
        .iter()
        .filter(|m| m.source == ModelSource::Vibe)
        .count();

    match engine {
        Some(e) => {
            let mut parts = vec![format!("moteur {}", e.label())];
            if ow_models > 0 {
                parts.push(format!("{ow_models} modèle(s) OpenWhispr"));
            } else if ow {
                parts.push("OpenWhispr installé".into());
            }
            if vibe_models > 0 {
                parts.push(format!("{vibe_models} modèle(s) Vibe"));
            } else if vibe {
                parts.push("Vibe installé".into());
            }
            parts.join(" — ")
        }
        None => {
            if ow || vibe {
                format!(
                    "modèles détectés (OpenWhispr={ow}, Vibe={vibe}) mais aucun moteur CLI (whisper-cli / vibe-server)"
                )
            } else {
                "Whisper / Vibe / OpenWhispr introuvable".into()
            }
        }
    }
}

fn normalize_lang(language: &str) -> Option<String> {
    if language.is_empty() || language == "auto" {
        return None;
    }
    let lang = match language {
        "french" => "fr",
        "english" => "en",
        "german" => "de",
        "spanish" => "es",
        "italian" => "it",
        other => other,
    };
    Some(lang.to_string())
}

#[derive(Debug, Clone)]
struct TimedSegment {
    start_ms: u64,
    end_ms: u64,
    text: String,
}

fn parse_timestamp_to_ms(raw: &str) -> Option<u64> {
    let s = raw.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, sec_part) = match parts.as_slice() {
        [sec] => (0u64, 0u64, *sec),
        [m, sec] => (0, m.parse().ok()?, *sec),
        [h, m, sec] => (h.parse().ok()?, m.parse().ok()?, *sec),
        _ => return None,
    };
    let sec_parts: Vec<&str> = sec_part.split('.').collect();
    let secs: u64 = sec_parts.first()?.parse().ok()?;
    let frac = sec_parts.get(1).copied().unwrap_or("0");
    let ms_str = format!("{:0<3}", &frac[..frac.len().min(3)]);
    let ms: u64 = ms_str.parse().ok()?;
    Some(h * 3_600_000 + m * 60_000 + secs * 1_000 + ms)
}

fn format_ts_srt(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1_000;
    let milli = ms % 1_000;
    format!("{h:02}:{m:02}:{s:02},{milli:03}")
}

fn format_ts_vtt(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1_000;
    let milli = ms % 1_000;
    format!("{h:02}:{m:02}:{s:02}.{milli:03}")
}

/// Parse vibe `--word-timestamps` lines: `[00:00.500 --> 00:00.900] hello`
fn parse_timed_stdout(stdout: &str) -> Vec<TimedSegment> {
    let re = regex::Regex::new(
        r"(?m)^\s*\[?\s*(\d{1,2}:\d{2}(?::\d{2})?[.,]\d{1,3})\s*-->\s*(\d{1,2}:\d{2}(?::\d{2})?[.,]\d{1,3})\s*\]?\s*(.*\S)\s*$",
    )
    .expect("regex");
    let mut out = Vec::new();
    for cap in re.captures_iter(stdout) {
        let Some(start_ms) = parse_timestamp_to_ms(&cap[1]) else {
            continue;
        };
        let Some(end_ms) = parse_timestamp_to_ms(&cap[2]) else {
            continue;
        };
        let text = cap[3].trim().to_string();
        if text.is_empty() {
            continue;
        }
        out.push(TimedSegment {
            start_ms,
            end_ms: end_ms.max(start_ms),
            text,
        });
    }
    out
}

/// JSON compatible Vibe / plugin universal-timestamp :
/// `[{ "start": 0.5, "stop": 1.2, "text": " mot" }, …]`
fn serialize_vibe_json(segments: &[TimedSegment]) -> String {
    let arr: Vec<serde_json::Value> = segments
        .iter()
        .map(|seg| {
            serde_json::json!({
                "start": (seg.start_ms as f64) / 1000.0,
                "stop": (seg.end_ms as f64) / 1000.0,
                "text": seg.text,
            })
        })
        .collect();
    // Indentation tab comme serializeVibeTranscript du plugin
    let mut out = String::from("[\n");
    for (i, v) in arr.iter().enumerate() {
        let start = v.get("start").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let stop = v.get("stop").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("");
        let text_esc = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
        out.push_str(&format!(
            "\t{{\n\t\t\"start\": {start},\n\t\t\"stop\": {stop},\n\t\t\"text\": {text_esc}\n\t}}"
        ));
        if i + 1 < arr.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("]\n");
    out
}

fn segments_from_whisper_oj(raw: &str) -> Vec<TimedSegment> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let Some(arr) = v.get("transcription").and_then(|t| t.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in arr {
        let text = item
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        if text.trim().is_empty() {
            continue;
        }
        let (start_ms, end_ms) = if let Some(off) = item.get("offsets") {
            let from = off.get("from").and_then(|x| x.as_u64()).unwrap_or(0);
            let to = off.get("to").and_then(|x| x.as_u64()).unwrap_or(from);
            (from, to.max(from))
        } else {
            (0, 0)
        };
        out.push(TimedSegment {
            start_ms,
            end_ms,
            text,
        });
    }
    out
}

fn segments_full_text(segments: &[TimedSegment]) -> String {
    let mut s = String::new();
    for seg in segments {
        let t = seg.text.trim();
        if t.is_empty() {
            continue;
        }
        if !s.is_empty() && !t.starts_with([',', '.', ';', ':', '!', '?', '…']) {
            s.push(' ');
        }
        s.push_str(t);
    }
    s
}

fn write_segments_as_format(
    segments: &[TimedSegment],
    plain_fallback: &str,
    out_base: &Path,
    format: TranscriptFormat,
) -> Result<PathBuf, WhisperError> {
    let result = out_base.with_extension(format.as_str());
    if let Some(parent) = result.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let body = if segments.is_empty() {
        match format {
            TranscriptFormat::Txt => plain_fallback.trim().to_string() + "\n",
            TranscriptFormat::Json => {
                // Format Vibe / Universal Timestamp : tableau [{start, stop, text}]
                let t = plain_fallback.trim();
                if t.is_empty() {
                    "[]\n".into()
                } else {
                    serialize_vibe_json(&[TimedSegment {
                        start_ms: 0,
                        end_ms: 0,
                        text: t.to_string(),
                    }])
                }
            }
            TranscriptFormat::Srt => {
                let t = plain_fallback.trim();
                if t.is_empty() {
                    String::new()
                } else {
                    format!("1\n00:00:00,000 --> 00:59:59,000\n{t}\n")
                }
            }
            TranscriptFormat::Vtt => {
                let t = plain_fallback.trim();
                if t.is_empty() {
                    "WEBVTT\n\n".into()
                } else {
                    format!("WEBVTT\n\n00:00:00.000 --> 00:59:59.000\n{t}\n")
                }
            }
        }
    } else {
        let full = segments_full_text(segments);
        match format {
            TranscriptFormat::Txt => full + "\n",
            TranscriptFormat::Json => {
                let _ = full;
                serialize_vibe_json(segments)
            }
            TranscriptFormat::Srt => {
                let mut srt = String::new();
                for (i, seg) in segments.iter().enumerate() {
                    srt.push_str(&format!(
                        "{}\n{} --> {}\n{}\n\n",
                        i + 1,
                        format_ts_srt(seg.start_ms),
                        format_ts_srt(seg.end_ms),
                        seg.text.trim()
                    ));
                }
                srt
            }
            TranscriptFormat::Vtt => {
                let mut vtt = String::from("WEBVTT\n\n");
                for seg in segments {
                    vtt.push_str(&format!(
                        "{} --> {}\n{}\n\n",
                        format_ts_vtt(seg.start_ms),
                        format_ts_vtt(seg.end_ms),
                        seg.text.trim()
                    ));
                }
                vtt
            }
        }
    };
    std::fs::write(&result, body)?;
    Ok(result)
}

fn transcribe_whisper_cli(
    engine: &ResolvedEngine,
    wav: &Path,
    model_path: &Path,
    out_base: &Path,
    params: &WhisperParams,
) -> Result<PathBuf, WhisperError> {
    let mut args = vec![
        "-m".into(),
        model_path.to_string_lossy().into_owned(),
        "-f".into(),
        wav.to_string_lossy().into_owned(),
        "-of".into(),
        out_base.to_string_lossy().into_owned(),
        "-np".into(),
    ];
    match params.format {
        TranscriptFormat::Srt => args.push("-osrt".into()),
        TranscriptFormat::Vtt => args.push("-ovtt".into()),
        TranscriptFormat::Txt => args.push("-otxt".into()),
        TranscriptFormat::Json => args.push("-oj".into()),
    }
    if let Some(lang) = normalize_lang(&params.language) {
        args.push("-l".into());
        args.push(lang);
    }

    tracing::info!(
        "Whisper CLI: {} → {:?}",
        wav.display(),
        params.format
    );
    let mut cmd = Command::new(&engine.path);
    cmd.args(&args);
    configure_no_window(&mut cmd);
    let output = cmd.output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(WhisperError::Failed(err.trim().to_string()));
    }

    let result = out_base.with_extension(params.format.as_str());
    // Convertir le JSON whisper.cpp natif → format Vibe [{start,stop,text}]
    if params.format == TranscriptFormat::Json {
        let oj = if result.is_file() {
            result.clone()
        } else {
            let alt = PathBuf::from(format!("{}.json", out_base.display()));
            if alt.is_file() {
                alt
            } else {
                return Err(WhisperError::Failed(format!(
                    "fichier de sortie manquant: {}",
                    result.display()
                )));
            }
        };
        let raw = std::fs::read_to_string(&oj)?;
        let segments = segments_from_whisper_oj(&raw);
        if segments.is_empty() {
            // Déjà au format Vibe, ou texte seul
            if parse_vibe_array(&raw).is_some() {
                if oj != result {
                    std::fs::rename(&oj, &result)?;
                }
                return Ok(result);
            }
            return write_segments_as_format(&[], &raw, out_base, TranscriptFormat::Json);
        }
        let body = serialize_vibe_json(&segments);
        std::fs::write(&result, body)?;
        if oj != result {
            let _ = std::fs::remove_file(&oj);
        }
        return Ok(result);
    }

    if result.is_file() {
        return Ok(result);
    }
    let alt = PathBuf::from(format!(
        "{}.{}",
        out_base.display(),
        params.format.as_str()
    ));
    if alt.is_file() {
        return Ok(alt);
    }
    Err(WhisperError::Failed(format!(
        "fichier de sortie manquant: {}",
        result.display()
    )))
}

fn parse_vibe_array(raw: &str) -> Option<()> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let arr = v.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let first = arr.first()?.as_object()?;
    if first.contains_key("start") && first.contains_key("stop") && first.contains_key("text") {
        Some(())
    } else {
        None
    }
}

fn transcribe_vibe_server(
    engine: &ResolvedEngine,
    audio: &Path,
    model_path: &Path,
    out_base: &Path,
    params: &WhisperParams,
) -> Result<PathBuf, WhisperError> {
    let mut cmd = Command::new(&engine.path);
    configure_no_window(&mut cmd);
    cmd.arg("transcribe")
        .arg(model_path)
        .arg(audio);
    if let Some(lang) = normalize_lang(&params.language) {
        cmd.arg("-l").arg(lang);
    }
    // Timecodes mots/phrases pour JSON / SRT / VTT
    let want_timestamps = matches!(
        params.format,
        TranscriptFormat::Json | TranscriptFormat::Srt | TranscriptFormat::Vtt
    );
    if want_timestamps {
        cmd.arg("--word-timestamps");
    }
    if let Some(dir) = &engine.work_dir {
        cmd.current_dir(dir);
    }
    // Help vibe find its bundled ffmpeg
    if let Some(dir) = &engine.work_dir {
        let ffmpeg = dir.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
        if ffmpeg.is_file() {
            cmd.env("VIBE_SERVER_FFMPEG_PATH", &ffmpeg);
            let sep = if cfg!(windows) { ";" } else { ":" };
            cmd.env(
                "PATH",
                format!(
                    "{}{sep}{}",
                    dir.display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        }
    }

    tracing::info!(
        "Vibe server: {} → {:?} (timestamps={want_timestamps})",
        audio.display(),
        params.format
    );
    let output = cmd.output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        return Err(WhisperError::Failed(
            if err.trim().is_empty() {
                out.trim().to_string()
            } else {
                err.trim().to_string()
            },
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let segments = if want_timestamps {
        parse_timed_stdout(&text)
    } else {
        Vec::new()
    };
    if want_timestamps && segments.is_empty() && !text.trim().is_empty() {
        tracing::warn!(
            "Vibe n’a pas renvoyé de timecodes parsables — JSON sans segments"
        );
    }
    write_segments_as_format(&segments, &text, out_base, params.format)
}

pub fn transcribe(
    input: &Path,
    output_without_ext: &Path,
    params: &WhisperParams,
    data_dir: Option<&Path>,
) -> Result<PathBuf, WhisperError> {
    let engine = resolve_whisper_engine(&params.cli_path).ok_or(WhisperError::CliNotFound)?;
    let (model_path, model_src) = resolve_model(&params.model)?;
    tracing::info!(
        "Modèle Whisper: {} ({})",
        model_path.display(),
        match model_src {
            ModelSource::OpenWhispr => "OpenWhispr",
            ModelSource::Vibe => "Vibe",
            ModelSource::CompressSpeak => "CompressSpeak",
            ModelSource::Downloaded => "téléchargé",
        }
    );

    let out_base = output_without_ext.to_path_buf();
    if let Some(parent) = out_base.parent() {
        std::fs::create_dir_all(parent)?;
    }

    match engine.kind {
        EngineKind::WhisperCli => {
            let ffmpeg =
                resolve_ffmpeg(data_dir).map_err(|e| WhisperError::Ffmpeg(e.to_string()))?;
            let tmp = tempfile::tempdir()?;
            let wav = tmp.path().join("audio_16k.wav");
            let cmd = build_mono_wav_command(&ffmpeg, input, &wav, 16000);
            run_command(&cmd).map_err(|e| WhisperError::Ffmpeg(e.to_string()))?;
            transcribe_whisper_cli(&engine, &wav, &model_path, &out_base, params)
        }
        EngineKind::VibeServer => {
            // vibe-server accepts many formats via its own ffmpeg; still prefer wav for reliability
            let audio_path = {
                match resolve_ffmpeg(data_dir) {
                    Ok(ffmpeg) => {
                        let tmp = tempfile::tempdir()?;
                        let wav = tmp.path().join("audio_16k.wav");
                        let cmd = build_mono_wav_command(&ffmpeg, input, &wav, 16000);
                        if run_command(&cmd).is_ok() {
                            // Keep tmp alive by copying next to out (tempdir would drop)
                            let kept = out_base.with_extension("cs_tmp_16k.wav");
                            std::fs::copy(&wav, &kept)?;
                            kept
                        } else {
                            input.to_path_buf()
                        }
                    }
                    Err(_) => input.to_path_buf(),
                }
            };
            let result = transcribe_vibe_server(&engine, &audio_path, &model_path, &out_base, params);
            if audio_path.extension().and_then(|e| e.to_str()) == Some("wav")
                && audio_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with("cs_tmp_16k.wav"))
            {
                let _ = std::fs::remove_file(&audio_path);
            }
            result
        }
    }
}
