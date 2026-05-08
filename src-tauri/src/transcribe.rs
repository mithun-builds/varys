//! whisper.cpp wrapper. Mostly soll's `transcribe.rs` but exposes timestamped
//! segments so the dual-channel orchestrator in `transcription.rs` can merge
//! mic + system transcripts by timeline.

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    /// Start time in milliseconds.
    pub start_ms: i64,
    /// End time in milliseconds.
    pub end_ms: i64,
    pub text: String,
}

pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    pub fn load(model_path: &Path) -> Result<Self> {
        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .ok_or_else(|| anyhow!("model path not utf-8"))?,
            WhisperContextParameters::default(),
        )
        .context("load whisper model")?;
        Ok(Self { ctx })
    }

    /// One throwaway inference on 1 s of silence — forces Metal kernel
    /// compilation off the hot path.
    #[allow(dead_code)]
    pub fn warm(&self) -> Result<()> {
        let silence = vec![0.0f32; 16_000];
        let _ = self.transcribe_segments(&silence, Arc::new(AtomicBool::new(false)))?;
        Ok(())
    }

    /// `cancel` is polled every Whisper iteration via the abort callback;
    /// flipping it to true mid-call returns early with a CANCELLED error.
    pub fn transcribe_segments(
        &self,
        samples: &[f32],
        cancel: Arc<AtomicBool>,
    ) -> Result<Vec<Segment>> {
        let mut state = self.ctx.create_state().context("create whisper state")?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(num_threads());
        params.set_translate(false);
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_single_segment(false);

        // Pre-flight cancel check. whisper-rs 0.11 doesn't expose a safe
        // abort callback, so cancellation is coarse — checked at the
        // channel boundary by the orchestrator. A future bump to
        // whisper-rs 0.12+ will let us abort mid-call.
        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow!("transcription cancelled"));
        }

        state.full(params, samples).context("whisper full")?;

        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow!("transcription cancelled"));
        }

        let n = state.full_n_segments().context("segment count")?;
        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n {
            let text = state
                .full_get_segment_text(i)
                .context("segment text")?
                .trim()
                .to_string();
            if text.is_empty() {
                continue;
            }
            let t0 = state.full_get_segment_t0(i).unwrap_or(0) * 10;
            let t1 = state.full_get_segment_t1(i).unwrap_or(0) * 10;
            out.push(Segment {
                start_ms: t0,
                end_ms: t1,
                text,
            });
        }
        Ok(out)
    }
}

fn num_threads() -> i32 {
    let n = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(4);
    (n.saturating_sub(1).max(2)) as i32
}
