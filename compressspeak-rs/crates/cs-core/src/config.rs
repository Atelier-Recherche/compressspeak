use cs_dpdf::DpdfParams;
use cs_ffmpeg::ConversionParams;
use cs_uvr5::Uvr5Params;
use cs_warmth::WarmthParams;
use cs_whisper::{TranscriptFormat, WhisperParams};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const APP_NAME: &str = "Compress Speak";
pub const ORG: &str = "CompressSpeak";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemePref {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Fr,
    En,
    De,
    Es,
}

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Self::Fr => "fr",
            Self::En => "en",
            Self::De => "de",
            Self::Es => "es",
        }
    }

    pub fn from_code(s: &str) -> Self {
        match s {
            "en" => Self::En,
            "de" => Self::De,
            "es" => Self::Es,
            _ => Self::Fr,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: ThemePref,
    pub language: Language,
    pub conversion: ConversionParams,
    pub warmth: WarmthParams,
    pub dpdf: DpdfParams,
    pub whisper: WhisperParams,
    pub uvr5: Uvr5Params,
    pub clean_suffix: String,
    pub keep_flac: bool,
    pub output_dir: String,
    pub default_gain_db: i32,
    /// Afficher le journal détaillé (masqué par défaut).
    #[serde(default)]
    pub show_logs: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: ThemePref::Dark,
            language: Language::Fr,
            conversion: ConversionParams::default(),
            warmth: WarmthParams::default(),
            dpdf: DpdfParams::default(),
            whisper: WhisperParams {
                model: "small".into(),
                language: "french".into(),
                format: TranscriptFormat::Txt,
                cli_path: String::new(),
            },
            uvr5: Uvr5Params::default(),
            clean_suffix: "_clean".into(),
            keep_flac: false,
            output_dir: String::new(),
            default_gain_db: 18,
            show_logs: false,
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let mut cfg: Self = confy::load(ORG, Some("config")).unwrap_or_default();
        cfg.dpdf.normalize();
        cfg
    }

    pub fn save(&self) -> Result<(), confy::ConfyError> {
        let mut cfg = self.clone();
        cfg.dpdf.normalize();
        confy::store(ORG, Some("config"), cfg)
    }
}

pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", ORG, APP_NAME)
        .map(|d| d.data_local_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub const AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "flac", "aac", "ogg", "opus", "m4a", "mp4",
];

pub fn is_audio(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.iter().any(|a| e.eq_ignore_ascii_case(a)))
        .unwrap_or(false)
}
