//! Dual-channel transcription orchestrator.
//!
//! Reads a stereo WAV (left = mic, right = system audio), transcribes each
//! channel independently with whisper.cpp, merges segments by timestamp,
//! and writes a labelled markdown transcript + a structured JSON file beside
//! the WAV.
//!
//! Outputs:
//!   - `<recording>.txt`  — readable transcript like `[00:01] You: …`
//!   - `<recording>.json` — `{ "segments": [{ "speaker", "start_ms", "end_ms", "text" }, …] }`

use crate::model::{self, WhisperModel};
use crate::transcribe::{Segment, Transcriber};
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::AppHandle;

pub const TARGET_SR: u32 = 16_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Speaker {
    /// Microphone — what the local user said.
    You,
    /// System audio — the other meeting participants.
    Them,
}

impl Speaker {
    fn label(&self) -> &'static str {
        match self {
            Speaker::You => "You",
            Speaker::Them => "Them",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LabelledSegment {
    pub speaker: Speaker,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptDocument {
    pub source_wav: String,
    pub model: String,
    pub segments: Vec<LabelledSegment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptionState {
    Idle,
    DownloadingModel { model: String, done_bytes: u64, total_bytes: u64 },
    LoadingModel,
    Transcribing { progress_pct: u8 },
    Cancelled,
    Done { transcript_path: String },
    Failed { message: String },
}

pub type SharedStatus = Arc<parking_lot::Mutex<TranscriptionState>>;

pub fn new_status() -> SharedStatus {
    Arc::new(parking_lot::Mutex::new(TranscriptionState::Idle))
}

pub async fn transcribe_recording(
    app: AppHandle,
    wav_path: PathBuf,
    model: WhisperModel,
    status: SharedStatus,
    cancel: Arc<AtomicBool>,
) -> Result<PathBuf> {
    // Reset cancel flag at the start of each new job.
    cancel.store(false, Ordering::SeqCst);

    let result =
        transcribe_recording_inner(&app, &wav_path, model, status.clone(), cancel.clone()).await;
    match &result {
        Ok(p) => {
            *status.lock() = TranscriptionState::Done {
                transcript_path: p.to_string_lossy().to_string(),
            };
            log::info!("transcript saved → {}", p.display());
        }
        Err(e) => {
            let msg = format!("{e:#}");
            *status.lock() = if cancel.load(Ordering::SeqCst) || msg.contains("cancelled") {
                TranscriptionState::Cancelled
            } else {
                TranscriptionState::Failed { message: msg }
            };
            log::error!("transcription failed: {e:?}");
        }
    }
    result
}

async fn transcribe_recording_inner(
    app: &AppHandle,
    wav_path: &Path,
    model: WhisperModel,
    status: SharedStatus,
    cancel: Arc<AtomicBool>,
) -> Result<PathBuf> {
    if !model::is_cached(app, model) {
        *status.lock() = TranscriptionState::DownloadingModel {
            model: model.id().to_string(),
            done_bytes: 0,
            total_bytes: 0,
        };
        let status_dl = status.clone();
        let model_id = model.id().to_string();
        model::ensure_model(app, model, move |done, total| {
            *status_dl.lock() = TranscriptionState::DownloadingModel {
                model: model_id.clone(),
                done_bytes: done,
                total_bytes: total,
            };
        })
        .await
        .context("download whisper model")?;
    }

    if cancel.load(Ordering::SeqCst) {
        return Err(anyhow!("transcription cancelled"));
    }

    *status.lock() = TranscriptionState::LoadingModel;
    let model_path = model::model_path(app, model)?;
    let transcriber = tokio::task::spawn_blocking(move || Transcriber::load(&model_path))
        .await
        .context("join transcriber load")??;

    *status.lock() = TranscriptionState::Transcribing { progress_pct: 0 };
    let (mic_samples, sys_samples) = tokio::task::spawn_blocking({
        let wav_path = wav_path.to_path_buf();
        move || split_stereo_to_mono(&wav_path)
    })
    .await
    .context("read WAV")??;

    let transcriber = Arc::new(transcriber);

    *status.lock() = TranscriptionState::Transcribing { progress_pct: 5 };
    let mic_segments = {
        let t = transcriber.clone();
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || t.transcribe_segments(&mic_samples, cancel))
            .await
            .context("join mic transcribe")??
    };

    if cancel.load(Ordering::SeqCst) {
        return Err(anyhow!("transcription cancelled"));
    }

    *status.lock() = TranscriptionState::Transcribing { progress_pct: 50 };
    let sys_segments = {
        let t = transcriber.clone();
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || t.transcribe_segments(&sys_samples, cancel))
            .await
            .context("join system transcribe")??
    };
    *status.lock() = TranscriptionState::Transcribing { progress_pct: 95 };

    let labelled = merge_segments(&mic_segments, &sys_segments);

    let txt_path = wav_path.with_extension("txt");
    let json_path = wav_path.with_extension("json");

    let txt = render_markdown(&labelled);
    tokio::fs::write(&txt_path, txt.as_bytes())
        .await
        .with_context(|| format!("write {}", txt_path.display()))?;

    let doc = TranscriptDocument {
        source_wav: wav_path.to_string_lossy().to_string(),
        model: model.id().to_string(),
        segments: labelled,
    };
    let json = serde_json::to_string_pretty(&doc).context("serialise json")?;
    tokio::fs::write(&json_path, json.as_bytes())
        .await
        .with_context(|| format!("write {}", json_path.display()))?;

    Ok(txt_path)
}

fn split_stereo_to_mono(wav_path: &Path) -> Result<(Vec<f32>, Vec<f32>)> {
    let mut reader = hound::WavReader::open(wav_path)
        .with_context(|| format!("open {}", wav_path.display()))?;
    let spec = reader.spec();
    if spec.channels < 1 {
        return Err(anyhow!("WAV has no channels"));
    }

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / i16::MAX as f32))
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("read int samples")?,
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("read float samples")?,
    };

    let channels = spec.channels as usize;
    let frames = interleaved.len() / channels;

    if spec.sample_rate != TARGET_SR {
        log::warn!(
            "WAV sample rate {} != {} — transcription accuracy may suffer",
            spec.sample_rate, TARGET_SR
        );
    }

    if channels == 1 {
        return Ok((interleaved.clone(), Vec::new()));
    }

    let mut mic = Vec::with_capacity(frames);
    let mut sys = Vec::with_capacity(frames);
    for f in 0..frames {
        mic.push(interleaved[f * channels]);
        sys.push(interleaved[f * channels + 1]);
    }
    Ok((mic, sys))
}

fn merge_segments(mic: &[Segment], sys: &[Segment]) -> Vec<LabelledSegment> {
    let mut out: Vec<LabelledSegment> = Vec::with_capacity(mic.len() + sys.len());
    for s in mic {
        out.push(LabelledSegment {
            speaker: Speaker::You,
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            text: s.text.clone(),
        });
    }
    for s in sys {
        out.push(LabelledSegment {
            speaker: Speaker::Them,
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            text: s.text.clone(),
        });
    }
    out.sort_by_key(|s| s.start_ms);
    out
}

fn render_markdown(segments: &[LabelledSegment]) -> String {
    let mut s = String::new();
    s.push_str("# Transcript\n\n");
    if segments.is_empty() {
        s.push_str("_No speech detected._\n");
        return s;
    }
    for seg in segments {
        let stamp = format_timestamp(seg.start_ms);
        s.push_str(&format!(
            "**[{stamp}] {speaker}:** {text}\n\n",
            speaker = seg.speaker.label(),
            text = seg.text.trim()
        ));
    }
    s
}

fn format_timestamp(ms: i64) -> String {
    let total_secs = (ms / 1000).max(0);
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let sec = total_secs % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{sec:02}")
    } else {
        format!("{m:02}:{sec:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: i64, end: i64, t: &str) -> Segment {
        Segment {
            start_ms: start,
            end_ms: end,
            text: t.into(),
        }
    }

    #[test]
    fn merge_orders_by_start() {
        let mic = vec![seg(0, 1000, "hello"), seg(3000, 4000, "ok thanks")];
        let sys = vec![seg(1500, 2500, "hi"), seg(5000, 6000, "bye")];
        let merged = merge_segments(&mic, &sys);
        assert_eq!(merged.len(), 4);
        assert_eq!(merged[0].speaker, Speaker::You);
        assert_eq!(merged[1].speaker, Speaker::Them);
        assert_eq!(merged[2].speaker, Speaker::You);
        assert_eq!(merged[3].speaker, Speaker::Them);
    }

    #[test]
    fn timestamp_under_hour() {
        assert_eq!(format_timestamp(0), "00:00");
        assert_eq!(format_timestamp(65_000), "01:05");
    }

    #[test]
    fn timestamp_over_hour() {
        assert_eq!(format_timestamp(3_661_000), "01:01:01");
    }
}
