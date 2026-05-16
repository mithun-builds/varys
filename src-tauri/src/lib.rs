mod audio_mic;
mod audio_mixer;
mod audio_system;
mod commands;
mod detection;
mod error;
mod metal;
mod model;
mod onboarding;
mod recording;
mod settings;
mod state;
mod storage;
mod transcribe;
mod transcription;
mod tray;

use std::sync::Arc;

use tauri::Manager;

use crate::state::AppState;

pub fn run() {
    // Must happen before whisper-rs spins up its Metal context. Lifted from
    // soll's lib.rs::run() — sets GGML_METAL_PATH_RESOURCES so whisper.cpp
    // finds the bundled Metal shader.
    metal::ensure_metal_resources();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::settings_general_get,
            commands::settings_set_output_folder,
            commands::settings_set_gains,
            commands::settings_set_auto_delete_days,
            commands::open_output_folder,
            commands::open_url,
            commands::app_version,
            commands::open_settings_window_cmd,
            commands::close_settings_window,
            commands::recording_status,
            commands::start_recording,
            commands::stop_recording,
            commands::transcription_status,
            commands::transcribe_existing,
            commands::cancel_transcription,
            commands::settings_set_whisper_model,
            commands::list_models,
            commands::download_model,
            commands::list_recordings,
            commands::open_path,
            commands::open_privacy_settings,
            commands::restart_app,
            commands::close_onboarding_window,
            onboarding::onboarding_status,
            onboarding::onboarding_dismiss,
            onboarding::request_mic_permission,
            onboarding::request_screen_recording_permission,
            onboarding::open_screen_recording_settings,
        ])
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            // macOS: tray-only app — no Dock icon. The Settings window opens
            // on demand from the tray menu.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let state = Arc::new(AppState::new(app.handle().clone())?);
            app.manage(state.clone());

            tray::build_tray(app.handle())?;

            // Initial onboarding state — open the wizard if any prereq is
            // missing AND the user hasn't already dismissed it. The dismissed
            // flag persists across launches so opted-out users aren't nagged,
            // but the tray badge stays on so they can re-open via the menu.
            let initial_complete = onboarding::onboarding_complete(app.handle(), &state);
            let dismissed = state
                .settings
                .get_bool(crate::settings::KEY_ONBOARDING_DISMISSED, false);
            tray::set_setup_needed(app.handle(), !initial_complete);
            if !initial_complete && !dismissed {
                tray::open_onboarding_window(app.handle());
            }

            // Auto-delete sweeper — runs at startup and every 6 hours after.
            // Reads `auto_delete_days` from settings each tick so changes to
            // the slider take effect on the next cycle without restart.
            // Setting 0 = disabled; the sweep short-circuits.
            let app_for_sweep = app.handle().clone();
            let state_for_sweep = state.clone();
            tauri::async_runtime::spawn(async move {
                use std::time::Duration;
                // Skip the very first tick by ~30s so we don't compete with
                // app launch + frontend boot for disk I/O.
                tokio::time::sleep(Duration::from_secs(30)).await;
                loop {
                    let days = state_for_sweep.auto_delete_days();
                    let out_dir = state_for_sweep.output_folder();
                    if days > 0 {
                        match storage::sweep_old_recordings(&out_dir, days) {
                            Ok(0) => {}
                            Ok(n) => log::info!("auto-delete: cleaned {n} stale files"),
                            Err(e) => log::warn!("auto-delete sweep: {e}"),
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(6 * 60 * 60)).await;
                    // Keep the app handle alive even when it's unused (e.g.
                    // when `days == 0`) so the sweeper can pick up a future
                    // toggle without restart.
                    let _ = &app_for_sweep;
                }
            });

            // Onboarding watcher — every 2 s, re-derive completeness; flip
            // the tray badge when state changes. Cheap; matches soll's cadence.
            let app_for_ob = app.handle().clone();
            let state_for_ob = state.clone();
            tauri::async_runtime::spawn(async move {
                use std::time::Duration;
                let mut prev = onboarding::onboarding_complete(&app_for_ob, &state_for_ob);
                loop {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    let current = onboarding::onboarding_complete(&app_for_ob, &state_for_ob);
                    if current != prev {
                        tray::set_setup_needed(&app_for_ob, !current);
                        prev = current;
                    }
                }
            });

            // Transcription state watcher — drives the tray icon's yellow
            // "transcribing" colour. Skips overriding the tray when a
            // recording is active (Recording wins visually since it's the
            // user's foreground task).
            let app_for_tx = app.handle().clone();
            let state_for_tx = state.clone();
            tauri::async_runtime::spawn(async move {
                use crate::transcription::TranscriptionState as TS;
                use std::time::Duration;
                let mut last_was_active = false;
                loop {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if state_for_tx.is_recording() {
                        last_was_active = false;
                        continue;
                    }
                    let s = state_for_tx.transcription_status.lock().clone();
                    let active = matches!(
                        s,
                        TS::DownloadingModel { .. }
                            | TS::LoadingModel
                            | TS::Transcribing { .. }
                    );
                    if active && !last_was_active {
                        tray::set_state(&app_for_tx, tray::TrayState::Transcribing);
                        last_was_active = true;
                    } else if !active && last_was_active {
                        tray::set_state(&app_for_tx, tray::TrayState::Idle);
                        last_was_active = false;
                    }
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Lord Varys")
        .run(|_app_handle, event| {
            // Tray app: only legitimate quit is the tray's Quit menu item,
            // which calls `app.exit(0)` (code = Some(0)). Ignore window-close
            // exit requests so closing Settings doesn't quit the app.
            if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
