//! Permission flow for the Settings → Permissions pane.
//!
//! Microphone: programmatic AVCaptureDevice prompt (lifted from soll). The OS
//! shows the TCC dialog the first time; subsequent calls are silent and just
//! return the cached status.
//!
//! Screen Recording: there is no programmatic prompt API. The only way to
//! surface the dialog is to actually invoke ScreenCaptureKit. The frontend
//! triggers `request_screen_recording_permission`, which spawns the Swift
//! sidecar in probe mode for ~500 ms; macOS shows the dialog on first
//! invocation and `CGPreflightScreenCaptureAccess` reports the result.

use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, State};

use crate::model;
use crate::settings::{
    KEY_HAS_RECORDED, KEY_MIC_PERMISSION_SEEN, KEY_ONBOARDING_DISMISSED, KEY_SCREEN_PERMISSION_SEEN,
};
use crate::state::AppState;

#[derive(Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PermState {
    Granted,
    Denied,
    Unknown,
}

#[derive(Serialize)]
pub struct OnboardingStatus {
    pub mic_permission: PermState,
    pub screen_permission: PermState,
    pub mic_seen: bool,
    pub screen_seen: bool,
    pub model_cached: bool,
    pub has_recorded: bool,
    pub dismissed: bool,
}

#[tauri::command]
pub fn onboarding_status(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> OnboardingStatus {
    let model = state.whisper_model();
    OnboardingStatus {
        mic_permission: check_mic_permission(),
        screen_permission: check_screen_recording_permission(),
        mic_seen: state.settings.get_bool(KEY_MIC_PERMISSION_SEEN, false),
        screen_seen: state.settings.get_bool(KEY_SCREEN_PERMISSION_SEEN, false),
        model_cached: model::is_cached(&app, model),
        has_recorded: state.settings.get_bool(KEY_HAS_RECORDED, false),
        dismissed: state.settings.get_bool(KEY_ONBOARDING_DISMISSED, false),
    }
}

/// True when all *required* prereqs are satisfied: mic + screen recording
/// granted, the active Whisper model is cached on disk. Drives whether the
/// onboarding window auto-opens on launch and whether the tray badge stays on.
pub fn onboarding_complete(app: &AppHandle, state: &AppState) -> bool {
    matches!(check_mic_permission(), PermState::Granted)
        && matches!(check_screen_recording_permission(), PermState::Granted)
        && model::is_cached(app, state.whisper_model())
}

#[tauri::command]
pub fn onboarding_dismiss(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    state
        .settings
        .set(KEY_ONBOARDING_DISMISSED, "true")
        .map_err(|e| e.to_string())?;
    crate::tray::set_setup_needed(&app, false);
    Ok(())
}

/// Trigger the macOS microphone permission dialog via AVFoundation.
///
/// Lifted from `soll/src-tauri/src/onboarding.rs` — uses
/// `AVCaptureDevice.requestAccessForMediaType:` because that's the only call
/// that surfaces the TCC dialog reliably on macOS 13+. Heap-allocates the
/// completion block via .copy() and forgets it so AVFoundation owns the
/// lifetime; we never read the granted bool here — the next status poll
/// picks it up.
#[tauri::command]
pub fn request_mic_permission(state: State<'_, Arc<AppState>>) {
    let _ = state.settings.set(KEY_MIC_PERMISSION_SEEN, "true");
    #[cfg(target_os = "macos")]
    unsafe {
        use block::ConcreteBlock;
        use objc::runtime::Class;
        use objc::{msg_send, sel, sel_impl};
        use std::os::raw::{c_char, c_void};

        extern "C" {
            fn dlopen(filename: *const c_char, flag: i32) -> *mut c_void;
        }
        dlopen(
            b"/System/Library/Frameworks/AVFoundation.framework/AVFoundation\0".as_ptr()
                as *const c_char,
            1,
        );

        let cls = match Class::get("AVCaptureDevice") {
            Some(c) => c,
            None => return,
        };
        let ns_cls = match Class::get("NSString") {
            Some(c) => c,
            None => return,
        };
        let media_type: *mut objc::runtime::Object = msg_send![
            ns_cls,
            stringWithUTF8String: b"soun\0".as_ptr() as *const c_char
        ];

        let block = ConcreteBlock::new(|_granted: bool| {});
        let block = block.copy();
        let _: () = msg_send![
            cls,
            requestAccessForMediaType: media_type
            completionHandler: &*block
        ];
        std::mem::forget(block);
    }
}

/// Open System Settings → Privacy → Screen Recording. macOS doesn't expose a
/// programmatic prompt for screen recording; the OS only fires the dialog
/// when SCKit is actually invoked. The Settings UI calls
/// `request_screen_recording_permission` first (which does invoke SCKit
/// briefly), and falls back to deeplinking the user here if the prompt was
/// previously denied.
#[tauri::command]
pub fn open_screen_recording_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn();
    }
}

/// Probe ScreenCaptureKit by spawning the sidecar in `--probe` mode for a
/// brief moment. macOS shows the consent dialog the first time SCKit is
/// invoked from this app's bundle; subsequent calls return immediately.
#[tauri::command]
pub async fn request_screen_recording_permission(
    state: State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    let _ = state.settings.set(KEY_SCREEN_PERMISSION_SEEN, "true");
    crate::audio_system::probe_permission()
        .await
        .map_err(|e| e.to_string())
}

// ── permission status checks ──────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub(crate) fn check_mic_permission() -> PermState {
    use objc::runtime::Class;
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        extern "C" {
            fn dlopen(
                filename: *const std::os::raw::c_char,
                flag: std::os::raw::c_int,
            ) -> *mut std::os::raw::c_void;
        }
        let path = b"/System/Library/Frameworks/AVFoundation.framework/AVFoundation\0";
        dlopen(path.as_ptr() as *const _, 1);
    }

    let status: i64 = unsafe {
        let cls = match Class::get("AVCaptureDevice") {
            Some(c) => c,
            None => return PermState::Unknown,
        };
        let ns_cls = match Class::get("NSString") {
            Some(c) => c,
            None => return PermState::Unknown,
        };
        let media_type: *mut objc::runtime::Object = msg_send![
            ns_cls,
            stringWithUTF8String: b"soun\0".as_ptr() as *const std::os::raw::c_char
        ];
        msg_send![cls, authorizationStatusForMediaType: media_type]
    };

    match status {
        3 => PermState::Granted,
        1 | 2 => PermState::Denied,
        _ => PermState::Unknown,
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn check_mic_permission() -> PermState {
    PermState::Granted
}

/// CGPreflightScreenCaptureAccess returns true once the user has granted the
/// app screen recording permission for this code-signed identity. Returns
/// false the first time (before the prompt fires) and after a deny.
#[cfg(target_os = "macos")]
pub(crate) fn check_screen_recording_permission() -> PermState {
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> u8;
    }
    if unsafe { CGPreflightScreenCaptureAccess() } != 0 {
        PermState::Granted
    } else {
        PermState::Unknown
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn check_screen_recording_permission() -> PermState {
    PermState::Granted
}

/// True when *all* required permissions are granted. Now superseded by
/// `onboarding_complete` (which also checks model cache); kept around for
/// the tray badge in legacy code paths.
#[allow(dead_code)]
pub fn all_permissions_granted() -> bool {
    matches!(check_mic_permission(), PermState::Granted)
        && matches!(check_screen_recording_permission(), PermState::Granted)
}
