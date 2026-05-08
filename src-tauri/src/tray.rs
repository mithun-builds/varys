//! Menu-bar tray UI. The tray is the only persistent surface — Settings opens
//! on demand. Icon flips between idle / recording / setup-needed states.
//!
//! Adapted from `soll/src-tauri/src/tray.rs` — the icon swap, status-line, and
//! recenter-window machinery are the same; the menu structure and state set
//! are slimmed down for the meeting-recording use case.

use anyhow::Result;
use once_cell::sync::{Lazy, OnceCell};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, Wry,
};

const TRAY_ID: &str = "lord-varys-tray";

pub const MENU_ID_TOGGLE: &str = "toggle_recording";
pub const MENU_ID_SETTINGS: &str = "settings";
pub const MENU_ID_ONBOARDING: &str = "onboarding";
pub const MENU_ID_QUIT: &str = "quit";

static IMG_IDLE: Lazy<Image<'static>> =
    Lazy::new(|| Image::from_bytes(include_bytes!("../icons/tray_idle.png")).unwrap());
static IMG_RECORDING: Lazy<Image<'static>> =
    Lazy::new(|| Image::from_bytes(include_bytes!("../icons/tray_recording.png")).unwrap());
static IMG_TRANSCRIBING: Lazy<Image<'static>> =
    Lazy::new(|| Image::from_bytes(include_bytes!("../icons/tray_transcribing.png")).unwrap());
static IMG_BADGE: Lazy<Image<'static>> =
    Lazy::new(|| Image::from_bytes(include_bytes!("../icons/tray_badge.png")).unwrap());

/// True while at least one onboarding step is unfinished. Drives the badge
/// icon independently of the recording state.
static SETUP_NEEDED: AtomicBool = AtomicBool::new(false);

static CURRENT_STATE: Lazy<Mutex<TrayState>> = Lazy::new(|| Mutex::new(TrayState::Idle));

/// Status line at top of the tray menu — kept as a `MenuItem` we can mutate.
static STATUS_ITEM: OnceCell<MenuItem<Wry>> = OnceCell::new();

/// "Start Recording" / "Stop Recording" menu item — label + enabled state get
/// flipped when the recording state changes.
static TOGGLE_ITEM: OnceCell<MenuItem<Wry>> = OnceCell::new();

/// Live handles to the menu so we can swap "Setup Guide…" on/off when the
/// user finishes onboarding (similar to soll's pattern).
static TRAY_MENU: Lazy<parking_lot::Mutex<Option<Menu<Wry>>>> =
    Lazy::new(|| parking_lot::Mutex::new(None));

#[derive(Copy, Clone, Debug)]
pub enum TrayState {
    Idle,
    Recording,
    Saving,
    Transcribing,
    PermissionMissing,
}

/// Kept as a stable enum for M2 when automatic detection returns. M1 only
/// uses `Unknown` since recording is manually triggered.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // GoogleMeet/Zoom/Teams variants unused until M2 detection
pub enum MeetingPlatform {
    GoogleMeet,
    Zoom,
    Teams,
    Unknown,
}

impl MeetingPlatform {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            MeetingPlatform::GoogleMeet => "Google Meet",
            MeetingPlatform::Zoom => "Zoom",
            MeetingPlatform::Teams => "Microsoft Teams",
            MeetingPlatform::Unknown => "meeting",
        }
    }
}

impl TrayState {
    fn status_text(&self) -> &'static str {
        match self {
            TrayState::Idle => "Idle",
            TrayState::Recording => "Recording…",
            TrayState::Saving => "Saving recording…",
            TrayState::Transcribing => "Transcribing…",
            TrayState::PermissionMissing => "Permissions needed — open Settings",
        }
    }

    fn tooltip(&self) -> &'static str {
        match self {
            TrayState::Idle => "Lord Varys — idle",
            TrayState::Recording => "Lord Varys — recording",
            TrayState::Saving => "Lord Varys — saving",
            TrayState::Transcribing => "Lord Varys — transcribing",
            TrayState::PermissionMissing => "Lord Varys — needs permissions",
        }
    }
}

pub fn build_tray(app: &AppHandle) -> Result<()> {
    let menu = build_menu(app)?;
    *TRAY_MENU.lock() = Some(menu.clone());
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(IMG_IDLE.clone())
        .icon_as_template(false)
        .tooltip(TrayState::Idle.tooltip())
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            match id {
                MENU_ID_QUIT => app.exit(0),
                MENU_ID_SETTINGS => open_settings_window(app),
                MENU_ID_ONBOARDING => open_onboarding_window(app),
                MENU_ID_TOGGLE => {
                    let app_clone = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = crate::commands::toggle_recording_internal(&app_clone).await
                        {
                            log::error!("toggle_recording: {e:?}");
                        }
                    });
                }
                _ => {}
            }
        })
        .build(app)?;

    set_state(app, TrayState::Idle);
    Ok(())
}

fn rebuild_menu(app: &AppHandle) {
    match build_menu(app) {
        Ok(menu) => {
            *TRAY_MENU.lock() = Some(menu.clone());
            if let Some(tray) = app.tray_by_id(TRAY_ID) {
                let _ = tray.set_menu(Some(menu));
            }
        }
        Err(e) => log::error!("rebuild_menu: {e:?}"),
    }
}

pub fn set_state(app: &AppHandle, state: TrayState) {
    *CURRENT_STATE.lock() = state;

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(state.tooltip()));
    }
    if let Some(item) = STATUS_ITEM.get() {
        let _ = item.set_text(state.status_text());
    }
    if let Some(item) = TOGGLE_ITEM.get() {
        let label = match state {
            TrayState::Idle | TrayState::PermissionMissing | TrayState::Transcribing => {
                "Start Recording"
            }
            TrayState::Recording => "Stop Recording",
            TrayState::Saving => "Saving…",
        };
        let _ = item.set_text(label);
        let _ = item.set_enabled(!matches!(state, TrayState::Saving));
    }
    apply_icon(app);
}

fn apply_icon(app: &AppHandle) {
    let state = *CURRENT_STATE.lock();
    let needs_setup = SETUP_NEEDED.load(Ordering::SeqCst);
    let img = match (state, needs_setup) {
        (_, true) => IMG_BADGE.clone(),
        (TrayState::PermissionMissing, _) => IMG_BADGE.clone(),
        (TrayState::Recording, _) => IMG_RECORDING.clone(),
        (TrayState::Saving, _) => IMG_RECORDING.clone(),
        (TrayState::Transcribing, _) => IMG_TRANSCRIBING.clone(),
        _ => IMG_IDLE.clone(),
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_icon(Some(img));
    }
}

pub fn set_setup_needed(app: &AppHandle, needed: bool) {
    let prev = SETUP_NEEDED.swap(needed, Ordering::SeqCst);
    if prev != needed {
        apply_icon(app);
        rebuild_menu(app);
    }
}

fn build_menu(app: &AppHandle) -> Result<Menu<Wry>> {
    let status_item = MenuItem::with_id(
        app,
        "status",
        TrayState::Idle.status_text(),
        false,
        None::<&str>,
    )?;
    let _ = STATUS_ITEM.set(status_item.clone());

    let toggle_item = MenuItem::with_id(
        app,
        MENU_ID_TOGGLE,
        "Start Recording",
        true,
        None::<&str>,
    )?;
    let _ = TOGGLE_ITEM.set(toggle_item.clone());

    let settings = MenuItem::with_id(app, MENU_ID_SETTINGS, "Settings…", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, MENU_ID_QUIT, "Quit Lord Varys", true, Some("Cmd+Q"))?;

    // Setup Guide entry only renders while onboarding is incomplete (matches
    // soll's pattern). Drops out of the menu cleanly once everything is set.
    let onboarding = if SETUP_NEEDED.load(Ordering::SeqCst) {
        Some(MenuItem::with_id(
            app,
            MENU_ID_ONBOARDING,
            "🔴  Setup Guide…",
            true,
            None::<&str>,
        )?)
    } else {
        None
    };

    let mut items: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = vec![
        &status_item,
        &toggle_item,
        &sep,
        &settings,
    ];
    if let Some(ref ob) = onboarding {
        items.push(ob);
    }
    items.push(&sep2);
    items.push(&quit);

    Menu::with_items(app, &items).map_err(Into::into)
}

pub fn open_settings_window(app: &AppHandle) {
    activate_app();
    let (cx, cy) = compute_center_position(app, 760.0, 600.0);

    if let Some(existing) = app.get_webview_window("settings") {
        let _ = existing.set_position(tauri::LogicalPosition::new(cx, cy));
        let _ = existing.show();
        let _ = existing.set_focus();
        return;
    }
    let url = WebviewUrl::App("index.html?view=settings".into());
    match WebviewWindowBuilder::new(app, "settings", url)
        .title("Lord Varys — Settings")
        .inner_size(760.0, 600.0)
        .min_inner_size(620.0, 480.0)
        .position(cx, cy)
        .visible(false)
        .resizable(true)
        .build()
    {
        Ok(window) => {
            let _ = window.show();
            let _ = window.set_focus();
            log::info!("opened settings window");
        }
        Err(e) => log::error!("open settings window: {e:?}"),
    }
}

pub fn open_onboarding_window(app: &AppHandle) {
    activate_app();
    let (cx, cy) = compute_center_position(app, 560.0, 680.0);

    if let Some(existing) = app.get_webview_window("onboarding") {
        let _ = existing.set_position(tauri::LogicalPosition::new(cx, cy));
        let _ = existing.show();
        let _ = existing.set_focus();
        return;
    }
    let url = WebviewUrl::App("index.html?view=onboarding".into());
    match WebviewWindowBuilder::new(app, "onboarding", url)
        .title("Lord Varys — Setup Guide")
        .inner_size(560.0, 680.0)
        .min_inner_size(440.0, 540.0)
        .position(cx, cy)
        .visible(false)
        .resizable(true)
        .build()
    {
        Ok(window) => {
            let _ = window.show();
            let _ = window.set_focus();
            log::info!("opened onboarding window");
        }
        Err(e) => log::error!("open onboarding window: {e:?}"),
    }
}

fn compute_center_position(app: &AppHandle, logical_w: f64, logical_h: f64) -> (f64, f64) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return (100.0, 100.0);
    };

    let mpos = monitor.position();
    let msize = monitor.size();
    let scale = monitor.scale_factor();
    let mlx = mpos.x as f64 / scale;
    let mly = mpos.y as f64 / scale;
    let mlw = msize.width as f64 / scale;
    let mlh = msize.height as f64 / scale;

    let x = mlx + ((mlw - logical_w) / 2.0).max(0.0);
    let y = mly + ((mlh - logical_h) / 2.0).max(0.0);
    (x, y)
}

#[allow(deprecated)]
fn activate_app() {
    #[cfg(target_os = "macos")]
    unsafe {
        use cocoa::appkit::NSApp;
        use objc::{msg_send, sel, sel_impl};
        let nsapp = NSApp();
        let _: () = msg_send![nsapp, activateIgnoringOtherApps: true];
    }
}
