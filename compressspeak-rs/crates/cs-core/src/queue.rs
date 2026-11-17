use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub path: PathBuf,
    pub gain_db: i32,
    pub isolate: bool,
    pub transcribe: bool,
    pub warmth: bool,
    pub dpdf: bool,
    pub progress: u8,
    pub status: ItemStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ItemStatus {
    #[default]
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl QueueItem {
    pub fn new(path: PathBuf, default_gain: i32) -> Self {
        Self {
            path,
            gain_db: default_gain,
            isolate: false,
            transcribe: false,
            warmth: false,
            dpdf: false,
            progress: 0,
            status: ItemStatus::Pending,
            error: None,
        }
    }

    pub fn display_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}
