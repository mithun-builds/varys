//! Meeting-event types, kept for M2 when automatic detection returns.
//! M1 ships with manual start/stop only — see `commands::start_recording`
//! and `commands::stop_recording`. The native_apps module also lives here
//! but is unused; restored when window-title gating is wired up.

#![allow(dead_code)]

use crate::tray::MeetingPlatform;
use serde::Serialize;
use tokio::sync::mpsc;

pub mod native_apps;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum DetectionSource {
    ChromeExt,
    NativePoll,
}

#[derive(Debug, Clone)]
pub enum MeetingEvent {
    Started {
        source: DetectionSource,
        platform: MeetingPlatform,
        title: String,
    },
    Ended {
        source: DetectionSource,
    },
}

pub type EventTx = mpsc::Sender<MeetingEvent>;
pub type EventRx = mpsc::Receiver<MeetingEvent>;

pub fn channel() -> (EventTx, EventRx) {
    mpsc::channel(64)
}
