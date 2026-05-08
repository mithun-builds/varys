//! `RecordingSession` — the active recording's start/stop orchestrator.
//!
//! Owns the mic recorder, the system-audio sidecar, and the mixer task.
//! `start` spawns all three and parks the mixer in a tokio task that runs
//! until both input streams close. `stop` signals the inputs to drain and
//! awaits the mixer task so the WAV is fully flushed before we return.

use crate::audio_mic::MicRecorder;
use crate::audio_mixer::{run_mixer, MixerConfig};
use crate::audio_system::SystemAudioRecorder;
use crate::error::{Error, Result};
use crate::onboarding::{check_mic_permission, check_screen_recording_permission, PermState};
use crate::state::AppState;
use crate::storage::{build_recording_path, with_degraded_suffix};
use crate::transcription;
use crate::tray::{self, MeetingPlatform, TrayState};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

pub struct RecordingSession {
    /// The mic recorder; `None` when mic permission was missing at start.
    mic: Option<MicRecorder>,
    /// The sidecar wrapper; `None` when screen recording permission was missing.
    sys: Option<SystemAudioRecorder>,
    /// Join handle for the mixer task. Awaited on stop().
    mixer_task: Option<tokio::task::JoinHandle<anyhow::Result<u64>>>,
    pub out_path: PathBuf,
    /// Reserved for M2 — filename templating that includes the platform
    /// name, and post-recording metadata writeback.
    #[allow(dead_code)]
    pub platform: MeetingPlatform,
}

impl RecordingSession {
    pub async fn start(
        app: &AppHandle,
        out_dir: PathBuf,
        title: &str,
        platform: MeetingPlatform,
        mic_gain: f32,
        sys_gain: f32,
    ) -> Result<Self> {
        let mic_perm = check_mic_permission();
        let scr_perm = check_screen_recording_permission();

        // Both denied → nothing to record. The detection layer should already
        // have flagged this case via the tray, but belt-and-braces here.
        if !matches!(mic_perm, PermState::Granted) && !matches!(scr_perm, PermState::Granted) {
            tray::set_state(app, TrayState::PermissionMissing);
            return Err(Error::PermissionRequired(
                "neither microphone nor screen recording is granted",
            ));
        }

        let (mic_rec_opt, mic_rx) = if matches!(mic_perm, PermState::Granted) {
            match MicRecorder::start() {
                Ok((rec, rx)) => (Some(rec), Some(rx)),
                Err(e) => {
                    log::warn!("mic capture failed: {e:?}");
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

        let (sys_rec_opt, sys_rx, sys_header) = if matches!(scr_perm, PermState::Granted) {
            match SystemAudioRecorder::start(app).await {
                Ok((rec, rx)) => {
                    let header = rec.header.clone();
                    (Some(rec), Some(rx), Some(header))
                }
                Err(e) => {
                    log::warn!("system audio capture failed: {e:?}");
                    (None, None, None)
                }
            }
        } else {
            (None, None, None)
        };

        // Reflect the actual capture mode in the filename so the user can
        // tell at a glance which streams made it onto the disk.
        let base = build_recording_path(&out_dir, title);
        let out_path = match (&mic_rec_opt, &sys_rec_opt) {
            (Some(_), Some(_)) => base,
            (Some(_), None) => with_degraded_suffix(&base, "mic-only"),
            (None, Some(_)) => with_degraded_suffix(&base, "sys-only"),
            (None, None) => {
                tray::set_state(app, TrayState::PermissionMissing);
                return Err(Error::Audio(
                    "neither input stream could be opened".into(),
                ));
            }
        };

        let mixer_cfg = MixerConfig {
            out_path: out_path.clone(),
            mic_gain,
            sys_gain,
            sys_header,
        };
        let task = tokio::spawn(async move { run_mixer(mixer_cfg, mic_rx, sys_rx).await });

        tray::set_state(app, TrayState::Recording);
        log::info!("recording started → {}", out_path.display());

        Ok(Self {
            mic: mic_rec_opt,
            sys: sys_rec_opt,
            mixer_task: Some(task),
            out_path,
            platform,
        })
    }

    pub async fn stop(mut self, app: &AppHandle) -> Result<PathBuf> {
        tray::set_state(app, TrayState::Saving);

        // Stopping mic + sidecar drops the senders inside, which lets the
        // mixer's recv() return None and the loop exit cleanly.
        if let Some(mic) = self.mic.take() {
            mic.stop();
        }
        if let Some(sys) = self.sys.take() {
            sys.stop().await;
        }

        if let Some(task) = self.mixer_task.take() {
            match task.await {
                Ok(Ok(n)) => log::info!("mixer finished: {n} samples"),
                Ok(Err(e)) => log::error!("mixer error: {e:?}"),
                Err(e) => log::error!("mixer task join failed: {e}"),
            }
        }

        tray::set_state(app, TrayState::Idle);

        // Auto-fire transcription. Don't await it — it's long-running, and
        // the user will poll status via the recording_status / transcription_status
        // commands. Errors land in the SharedStatus.
        if let Some(state) = app.try_state::<Arc<AppState>>() {
            let inner = state.inner().clone();
            let status = inner.transcription_status.clone();
            let cancel = inner.transcription_cancel.clone();
            let model = inner.whisper_model();
            let app_clone = app.clone();
            let wav_path = self.out_path.clone();
            tauri::async_runtime::spawn(async move {
                let _ = transcription::transcribe_recording(
                    app_clone, wav_path, model, status, cancel,
                )
                .await;
            });
        }

        Ok(self.out_path.clone())
    }
}
