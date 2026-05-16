//! Tauri command surface — every IPC call from the React frontend goes
//! through one of these. Keeps the boundary thin and well-typed.

use crate::error::Error;
use crate::model::{self, WhisperModel};
use crate::recording::RecordingSession;
use crate::settings::{
    KEY_AUTO_DELETE_DAYS, KEY_MIC_GAIN, KEY_OUTPUT_FOLDER, KEY_SYS_GAIN, KEY_WHISPER_MODEL,
};
use crate::state::AppState;
use crate::transcription::TranscriptionState;
use crate::tray::{self, MeetingPlatform};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

#[derive(Serialize)]
pub struct GeneralSettings {
    pub output_folder: String,
    pub mic_gain: f32,
    pub sys_gain: f32,
    pub whisper_model: String,
    pub auto_delete_days: u32,
}

#[tauri::command]
pub fn settings_general_get(state: State<'_, Arc<AppState>>) -> GeneralSettings {
    GeneralSettings {
        output_folder: state.output_folder().to_string_lossy().to_string(),
        mic_gain: state.mic_gain(),
        sys_gain: state.sys_gain(),
        whisper_model: state.whisper_model().id().to_string(),
        auto_delete_days: state.auto_delete_days(),
    }
}

#[tauri::command]
pub fn settings_set_auto_delete_days(
    state: State<'_, Arc<AppState>>,
    days: u32,
) -> Result<(), String> {
    // Cap at 10 years to keep nonsense out of the DB.
    let days = days.min(3650);
    state
        .settings
        .set(KEY_AUTO_DELETE_DAYS, &days.to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_set_whisper_model(
    state: State<'_, Arc<AppState>>,
    model: String,
) -> Result<(), String> {
    if WhisperModel::from_id(&model).is_none() {
        return Err(format!("unknown model: {model}"));
    }
    state
        .settings
        .set(KEY_WHISPER_MODEL, &model)
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub short_name: String,
    pub size_label: String,
    pub is_cached: bool,
    pub is_active: bool,
}

#[tauri::command]
pub fn list_models(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Vec<ModelInfo> {
    let active = state.whisper_model();
    [WhisperModel::TinyEn, WhisperModel::SmallEn, WhisperModel::MediumEn]
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id().to_string(),
            short_name: m.short_name().to_string(),
            size_label: m.size_label().to_string(),
            is_cached: model::is_cached(&app, m),
            is_active: m == active,
        })
        .collect()
}

/// Kick off a background download of `id` if not already cached. Status
/// surfaces via `transcription_status` (we reuse the same state to avoid
/// inventing a parallel download channel).
#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    let m = WhisperModel::from_id(&id).ok_or_else(|| format!("unknown model: {id}"))?;
    if model::is_cached(&app, m) {
        return Ok(());
    }
    let status = state.transcription_status.clone();
    let model_id = m.id().to_string();
    *status.lock() = TranscriptionState::DownloadingModel {
        model: model_id.clone(),
        done_bytes: 0,
        total_bytes: 0,
    };
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let status_dl = status.clone();
        let progress_id = model_id.clone();
        let result = model::ensure_model(&app_clone, m, move |done, total| {
            *status_dl.lock() = TranscriptionState::DownloadingModel {
                model: progress_id.clone(),
                done_bytes: done,
                total_bytes: total,
            };
        })
        .await;
        match result {
            Ok(_) => {
                *status.lock() = TranscriptionState::Idle;
                log::info!("model {model_id} downloaded");
            }
            Err(e) => {
                *status.lock() = TranscriptionState::Failed {
                    message: format!("model download: {e:#}"),
                };
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_transcription(state: State<'_, Arc<AppState>>) {
    state.transcription_cancel.store(true, Ordering::SeqCst);
    log::info!("transcription cancellation requested");
}

#[derive(Serialize)]
pub struct RecordingStatus {
    pub is_recording: bool,
    pub out_path: Option<String>,
}

#[tauri::command]
pub fn recording_status(state: State<'_, Arc<AppState>>) -> RecordingStatus {
    let guard = state.current_recording.lock();
    let out_path = guard.as_ref().map(|s| s.out_path.to_string_lossy().to_string());
    RecordingStatus {
        is_recording: guard.is_some(),
        out_path,
    }
}

#[tauri::command]
pub fn transcription_status(
    state: State<'_, Arc<AppState>>,
) -> crate::transcription::TranscriptionState {
    state.transcription_status.lock().clone()
}

#[tauri::command]
pub fn open_path(path: String) {
    let _ = std::process::Command::new("open").arg(&path).spawn();
}

/// Re-transcribe an existing WAV — useful for recordings made before the
/// transcription pipeline existed, or to re-run after a failure or model change.
#[tauri::command]
pub async fn transcribe_existing(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("WAV not found: {path}"));
    }
    let status = state.transcription_status.clone();
    let cancel = state.transcription_cancel.clone();
    let model = state.whisper_model();
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = crate::transcription::transcribe_recording(
            app_clone, p, model, status, cancel,
        )
        .await;
    });
    Ok(())
}

/// List WAVs in the configured output folder along with whether each has a
/// matching `.txt` transcript already on disk.
#[tauri::command]
pub fn list_recordings(state: State<'_, Arc<AppState>>) -> Vec<RecordingEntry> {
    let dir = state.output_folder();
    let mut entries: Vec<RecordingEntry> = Vec::new();
    if let Ok(read) = std::fs::read_dir(&dir) {
        for e in read.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("wav") {
                continue;
            }
            let txt = p.with_extension("txt");
            entries.push(RecordingEntry {
                wav_path: p.to_string_lossy().to_string(),
                file_name: p
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_string(),
                has_transcript: txt.exists(),
                transcript_path: if txt.exists() {
                    Some(txt.to_string_lossy().to_string())
                } else {
                    None
                },
            });
        }
    }
    // Newest first — easier to grab the recording you just made.
    entries.sort_by(|a, b| b.file_name.cmp(&a.file_name));
    entries
}

#[derive(Serialize)]
pub struct RecordingEntry {
    pub wav_path: String,
    pub file_name: String,
    pub has_transcript: bool,
    pub transcript_path: Option<String>,
}

#[tauri::command]
pub async fn start_recording(app: AppHandle, name: Option<String>) -> Result<(), String> {
    toggle_start(&app, name).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> Result<(), String> {
    toggle_stop(&app).await.map_err(|e| e.to_string())
}

/// Single entry point used by both the tray "Start/Stop Recording" menu
/// item and the frontend's manual buttons. Reads the current state and
/// flips it. Tray-initiated starts pass `None` for the name (falls back to
/// the "manual" slug); the Settings UI's named-start flow passes a real
/// string when the input field is non-empty.
pub async fn toggle_recording_internal(app: &AppHandle) -> Result<(), Error> {
    let state = match app.try_state::<Arc<AppState>>() {
        Some(s) => s.inner().clone(),
        None => return Err(Error::Audio("app state not ready".into())),
    };
    if state.is_recording() {
        toggle_stop(app).await
    } else {
        toggle_start(app, None).await
    }
}

async fn toggle_start(app: &AppHandle, name: Option<String>) -> Result<(), Error> {
    let state = app
        .try_state::<Arc<AppState>>()
        .ok_or_else(|| Error::Audio("app state not ready".into()))?
        .inner()
        .clone();
    if state.is_recording() {
        return Err(Error::AlreadyRecording);
    }
    let out_dir = state.output_folder();
    let mic_gain = state.mic_gain();
    let sys_gain = state.sys_gain();
    // Fall back to "manual" for empty / missing names so the filename always
    // has a meaningful slug suffix. Trim whitespace so a stray space doesn't
    // produce a weird filename.
    let title = name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("manual");
    let session = RecordingSession::start(
        app,
        out_dir,
        title,
        MeetingPlatform::Unknown,
        mic_gain,
        sys_gain,
    )
    .await?;
    *state.current_recording.lock() = Some(session);
    Ok(())
}

async fn toggle_stop(app: &AppHandle) -> Result<(), Error> {
    let state = app
        .try_state::<Arc<AppState>>()
        .ok_or_else(|| Error::Audio("app state not ready".into()))?
        .inner()
        .clone();
    let session = state.current_recording.lock().take();
    let session = session.ok_or(Error::NotRecording)?;
    let app_clone = app.clone();
    tokio::spawn(async move {
        if let Err(e) = session.stop(&app_clone).await {
            log::error!("stop recording failed: {e}");
        }
    });
    Ok(())
}

#[tauri::command]
pub fn settings_set_output_folder(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("output folder cannot be empty".into());
    }
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    state
        .settings
        .set(KEY_OUTPUT_FOLDER, &path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_set_gains(
    state: State<'_, Arc<AppState>>,
    mic_gain: f32,
    sys_gain: f32,
) -> Result<(), String> {
    state
        .settings
        .set(KEY_MIC_GAIN, &mic_gain.to_string())
        .map_err(|e| e.to_string())?;
    state
        .settings
        .set(KEY_SYS_GAIN, &sys_gain.to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_output_folder(state: State<'_, Arc<AppState>>) {
    let path = state.output_folder();
    let _ = std::process::Command::new("open").arg(&path).spawn();
}

#[tauri::command]
pub fn open_url(url: String) {
    let _ = std::process::Command::new("open").arg(&url).spawn();
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub fn open_settings_window_cmd(app: AppHandle) {
    tray::open_settings_window(&app);
}

#[tauri::command]
pub fn close_settings_window(app: AppHandle) {
    if let Some(window) = tauri::Manager::get_webview_window(&app, "settings") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn close_onboarding_window(app: AppHandle) {
    if let Some(window) = tauri::Manager::get_webview_window(&app, "onboarding") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn open_privacy_settings(section: String) {
    // Whitelist the section names we actually use to avoid arbitrary
    // x-apple.systempreferences URLs from the renderer.
    let suffix = match section.as_str() {
        "Privacy_Microphone" => "Privacy_Microphone",
        "Privacy_ScreenCapture" => "Privacy_ScreenCapture",
        _ => return,
    };
    let _ = std::process::Command::new("open")
        .arg(format!(
            "x-apple.systempreferences:com.apple.preference.security?{suffix}"
        ))
        .spawn();
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

