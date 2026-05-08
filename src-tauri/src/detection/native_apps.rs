//! Native-app meeting detection via NSWorkspace polling.
//!
//! **Disabled in M1.** The original "the app is running" heuristic turned out
//! to be way too aggressive: having Zoom open for chat or Teams open for
//! channels would fire `MeetingEvent::Started` and the tray would flip to
//! Recording with no actual call.
//!
//! M2 plan: keep the bundle-ID presence as a fast pre-filter, then gate on
//! `CGWindowListCopyWindowInfo` — emit `Started` only when there's a window
//! titled "Zoom Meeting" (Zoom) or matching Teams's in-call window pattern.
//! `CGWindowListCopyWindowInfo` requires Screen Recording permission to read
//! window titles on macOS 12.3+, which we already require for SCKit, so
//! there's no extra prompt cost.

#![allow(dead_code)] // entire module reserved for M2 — see header

use crate::detection::{DetectionSource, EventTx, MeetingEvent};
use crate::tray::MeetingPlatform;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

const WATCHED: &[(&str, MeetingPlatform)] = &[
    ("us.zoom.xos", MeetingPlatform::Zoom),
    ("com.microsoft.teams2", MeetingPlatform::Teams),
    ("com.microsoft.teams", MeetingPlatform::Teams),
];

pub async fn run(tx: EventTx) {
    let mut active: Option<MeetingPlatform> = None;

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        let running = scan_running_apps();
        let detected = WATCHED
            .iter()
            .find(|(bundle, _)| running.iter().any(|b| b == bundle))
            .map(|(_, p)| *p);

        match (active, detected) {
            (None, Some(p)) => {
                active = Some(p);
                let _ = tx
                    .send(MeetingEvent::Started {
                        source: DetectionSource::NativePoll,
                        platform: p,
                        title: String::new(),
                    })
                    .await;
            }
            (Some(_), None) => {
                active = None;
                let _ = tx
                    .send(MeetingEvent::Ended {
                        source: DetectionSource::NativePoll,
                    })
                    .await;
            }
            // Same platform still running, or both idle — no transition.
            _ => {}
        }
    }
}

#[cfg(target_os = "macos")]
fn scan_running_apps() -> Vec<String> {
    use objc::runtime::Class;
    use objc::{msg_send, sel, sel_impl};

    type Id = *mut objc::runtime::Object;
    let nil: Id = std::ptr::null_mut();

    unsafe {
        let cls = match Class::get("NSWorkspace") {
            Some(c) => c,
            None => return Vec::new(),
        };
        let workspace: Id = msg_send![cls, sharedWorkspace];
        let apps: Id = msg_send![workspace, runningApplications];
        if apps == nil {
            return Vec::new();
        }
        let count: usize = msg_send![apps, count];
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let app: Id = msg_send![apps, objectAtIndex: i];
            if app == nil {
                continue;
            }
            let bundle: Id = msg_send![app, bundleIdentifier];
            if bundle == nil {
                continue;
            }
            let cstr: *const std::os::raw::c_char = msg_send![bundle, UTF8String];
            if cstr.is_null() {
                continue;
            }
            if let Ok(s) = std::ffi::CStr::from_ptr(cstr).to_str() {
                out.push(s.to_string());
            }
        }
        out
    }
}

#[cfg(not(target_os = "macos"))]
fn scan_running_apps() -> Vec<String> {
    Vec::new()
}
