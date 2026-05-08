//! Mix microphone + system-audio streams into a stereo 16 kHz WAV.
//!
//! Channel layout: **left = mic, right = system**. Keeping the streams
//! separate (instead of summing to mono) lets the transcription layer run
//! Whisper on each side independently and label segments as You vs Them.
//!
//! Both inputs arrive on tokio mpsc channels:
//!   - mic: already 16 kHz mono f32 (audio_mic.rs resamples in the cpal callback).
//!   - system: 48 kHz interleaved stereo f32 from sckit_capture. Downmixed
//!     to mono and resampled to 16 kHz here.
//!
//! Time alignment is naive — we mix samples as they arrive, gating on
//! whichever side has data. Sub-200 ms drift is acceptable for downstream
//! transcription. A jitter buffer would be a future-quality improvement,
//! not a correctness fix.

use anyhow::{Context, Result};
use hound::{SampleFormat as HoundSampleFormat, WavSpec, WavWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::audio_system::StreamHeader;

const TARGET_SR: u32 = 16_000;

pub struct MixerConfig {
    pub out_path: PathBuf,
    pub mic_gain: f32,
    pub sys_gain: f32,
    pub sys_header: Option<StreamHeader>,
}

/// Run the mixer until both input channels close. Returns the per-channel
/// sample count written to disk.
pub async fn run_mixer(
    cfg: MixerConfig,
    mut mic_rx: Option<mpsc::Receiver<Vec<f32>>>,
    mut sys_rx: Option<mpsc::Receiver<Vec<f32>>>,
) -> Result<u64> {
    if let Some(parent) = cfg.out_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let spec = WavSpec {
        channels: 2, // left = mic, right = system
        sample_rate: TARGET_SR,
        bits_per_sample: 16,
        sample_format: HoundSampleFormat::Int,
    };
    let writer = WavWriter::new(
        BufWriter::new(File::create(&cfg.out_path).with_context(|| {
            format!("create output WAV at {}", cfg.out_path.display())
        })?),
        spec,
    )
    .context("init WAV writer")?;
    let writer = std::sync::Arc::new(parking_lot::Mutex::new(writer));

    // sys-side resampler state — same linear-interp shape as audio_mic.rs.
    let sys_src_rate = cfg
        .sys_header
        .as_ref()
        .map(|h| h.sample_rate)
        .unwrap_or(48_000);
    let sys_channels = cfg
        .sys_header
        .as_ref()
        .map(|h| h.channels)
        .unwrap_or(2);
    let sys_ratio = sys_src_rate as f64 / TARGET_SR as f64;
    let mut sys_accum: f64 = 0.0;
    let mut sys_last: f32 = 0.0;
    let mut sys_resampled: Vec<f32> = Vec::with_capacity(2048);

    let mut mic_buf: Vec<f32> = Vec::with_capacity(2048);
    let mut total_samples: u64 = 0;

    loop {
        tokio::select! {
            biased;
            chunk = async { match mic_rx.as_mut() { Some(r) => r.recv().await, None => None } }, if mic_rx.is_some() => {
                match chunk {
                    Some(c) => mic_buf.extend_from_slice(&c),
                    None => mic_rx = None,
                }
            }
            chunk = async { match sys_rx.as_mut() { Some(r) => r.recv().await, None => None } }, if sys_rx.is_some() => {
                match chunk {
                    Some(interleaved) => {
                        let mono = downmix_mono(&interleaved, sys_channels);
                        for &s in &mono {
                            let prev = sys_last;
                            sys_last = s;
                            sys_accum += 1.0;
                            while sys_accum >= sys_ratio {
                                let frac = ((sys_accum - sys_ratio) / sys_ratio) as f32;
                                let interp = prev + (sys_last - prev) * (1.0 - frac.clamp(0.0, 1.0));
                                sys_resampled.push(interp);
                                sys_accum -= sys_ratio;
                            }
                        }
                    }
                    None => sys_rx = None,
                }
            }
            else => break,
        }

        let mic_only = sys_rx.is_none();
        let sys_only = mic_rx.is_none();

        // How many *frames* (each frame = 1 mic sample + 1 sys sample) we
        // can emit right now. When one side is closed but the other still
        // has data, we keep writing; the missing side gets zeros.
        let frames = if mic_only && sys_only {
            mic_buf.len().max(sys_resampled.len())
        } else if mic_only {
            mic_buf.len()
        } else if sys_only {
            sys_resampled.len()
        } else {
            mic_buf.len().min(sys_resampled.len())
        };

        if frames == 0 {
            continue;
        }

        let mic_slice: Vec<f32> = mic_buf.drain(..frames.min(mic_buf.len())).collect();
        let sys_slice: Vec<f32> = sys_resampled.drain(..frames.min(sys_resampled.len())).collect();

        let mut w = writer.lock();
        for i in 0..frames {
            let m = mic_slice.get(i).copied().unwrap_or(0.0) * cfg.mic_gain;
            let s = sys_slice.get(i).copied().unwrap_or(0.0) * cfg.sys_gain;
            // Interleave L/R. Soft-clip via tanh (gentler than hard clamp
            // when gain > 1.0). Each channel kept independent so transcribe
            // can split them cleanly.
            let l = (m).tanh();
            let r = (s).tanh();
            w.write_sample((l.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).ok();
            w.write_sample((r.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).ok();
            total_samples += 1;
        }
    }

    let writer = std::sync::Arc::try_unwrap(writer)
        .map_err(|_| anyhow::anyhow!("mixer writer still has outstanding refs"))?
        .into_inner();
    writer.finalize().context("finalize WAV")?;
    log::info!(
        "mixer wrote {} stereo frames ({:.2}s) to {}",
        total_samples,
        total_samples as f64 / TARGET_SR as f64,
        cfg.out_path.display()
    );
    Ok(total_samples)
}

fn downmix_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let ch = channels as usize;
    interleaved
        .chunks_exact(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}
