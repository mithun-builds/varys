use anyhow::{Context, Result};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::model::WhisperModel;
use crate::recording::RecordingSession;
use crate::settings::{
    Settings, DEFAULT_MIC_GAIN, DEFAULT_SYS_GAIN, KEY_MIC_GAIN, KEY_OUTPUT_FOLDER, KEY_SYS_GAIN,
    KEY_WHISPER_MODEL,
};
use crate::storage;
use crate::transcription::{self, SharedStatus};
use std::sync::atomic::AtomicBool;

/// App-global state: settings DB, current recording session, latest
/// transcription job status, app handle.
pub struct AppState {
    /// Reserved — modules that hold a `SharedState` will need it to open
    /// windows or fire tray updates without threading an AppHandle through
    /// every call site.
    #[allow(dead_code)]
    pub app: AppHandle,
    pub settings: Settings,
    pub current_recording: Mutex<Option<RecordingSession>>,
    /// Status of the most-recent transcription job. Each new job replaces
    /// the previous; M2 exposes a single in-flight transcription at a time
    /// since whisper.cpp/Metal contends on one device anyway.
    pub transcription_status: SharedStatus,
    /// Set to `true` to abort the in-flight transcription. Reset to `false`
    /// at the start of each new job. Whisper's abort callback polls this.
    pub transcription_cancel: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(app: AppHandle) -> Result<Self> {
        let data_dir = app
            .path()
            .app_data_dir()
            .context("resolve app data dir")?;
        std::fs::create_dir_all(&data_dir).ok();

        let settings = Settings::open(&data_dir.join("settings.db"))?;
        seed_defaults(&settings)?;

        Ok(Self {
            app,
            settings,
            current_recording: Mutex::new(None),
            transcription_status: transcription::new_status(),
            transcription_cancel: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn output_folder(&self) -> PathBuf {
        let raw = self.settings.get_or_default(
            KEY_OUTPUT_FOLDER,
            storage::default_output_folder().to_str().unwrap_or("."),
        );
        PathBuf::from(raw)
    }

    pub fn mic_gain(&self) -> f32 {
        self.settings.get_f32(KEY_MIC_GAIN, DEFAULT_MIC_GAIN).clamp(0.0, 2.0)
    }

    pub fn sys_gain(&self) -> f32 {
        self.settings.get_f32(KEY_SYS_GAIN, DEFAULT_SYS_GAIN).clamp(0.0, 2.0)
    }

    pub fn is_recording(&self) -> bool {
        self.current_recording.lock().is_some()
    }

    pub fn whisper_model(&self) -> WhisperModel {
        let raw = self.settings.get_or_default(KEY_WHISPER_MODEL, WhisperModel::DEFAULT.id());
        WhisperModel::from_id(&raw).unwrap_or(WhisperModel::DEFAULT)
    }
}

fn seed_defaults(settings: &Settings) -> Result<()> {
    let default = storage::default_output_folder();

    let needs_seed = match settings.get(KEY_OUTPUT_FOLDER)? {
        None => true,
        Some(existing) => {
            // Migrate users off old defaults so changing the default in code
            // actually moves their recordings folder. We only overwrite when
            // the stored path matches a known old default — anything else is
            // a deliberate user choice we leave alone.
            let existing_path = PathBuf::from(&existing);
            storage::known_old_defaults()
                .iter()
                .any(|old| old == &existing_path)
        }
    };
    if needs_seed {
        std::fs::create_dir_all(&default).ok();
        settings.set(KEY_OUTPUT_FOLDER, default.to_str().unwrap_or("."))?;
    }

    if settings.get(KEY_MIC_GAIN)?.is_none() {
        settings.set(KEY_MIC_GAIN, &DEFAULT_MIC_GAIN.to_string())?;
    }
    if settings.get(KEY_SYS_GAIN)?.is_none() {
        settings.set(KEY_SYS_GAIN, &DEFAULT_SYS_GAIN.to_string())?;
    }
    Ok(())
}

#[allow(dead_code)]
pub type SharedState = Arc<AppState>;
