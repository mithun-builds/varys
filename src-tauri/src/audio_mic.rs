//! Microphone capture via cpal.
//!
//! Streaming variant of `soll/src-tauri/src/audio.rs`. Soll buffers every f32
//! sample into a `Vec` and processes it after stop(); we instead push small
//! chunks through an mpsc channel so the mixer can interleave with system
//! audio in (near) real time.
//!
//! cpal's `Stream` is `!Send` on macOS (it's tied to the CoreAudio thread),
//! so we keep it on a dedicated thread the same way soll does. Resampling to
//! 16 kHz mono happens inside the audio thread — keeps the channel payload
//! small and avoids racing the resampler against shutdown.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use std::sync::mpsc as std_mpsc;
use std::thread::JoinHandle;
use tokio::sync::mpsc as tokio_mpsc;

pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Chunk size pushed onto the channel. ~10 ms at 16 kHz — small enough that
/// the mixer doesn't introduce noticeable latency, large enough that channel
/// overhead stays negligible.
const CHUNK_SAMPLES: usize = 160;

pub struct MicRecorder {
    shutdown_tx: std_mpsc::SyncSender<()>,
    thread: Option<JoinHandle<()>>,
}

impl MicRecorder {
    /// Start mic capture. Returns the recorder handle and a tokio receiver
    /// yielding 16 kHz mono f32 chunks. The receiver closes when stop() is
    /// called or the audio device disappears.
    pub fn start() -> Result<(Self, tokio_mpsc::Receiver<Vec<f32>>)> {
        let (shutdown_tx, shutdown_rx) = std_mpsc::sync_channel::<()>(1);
        let (ready_tx, ready_rx) = std_mpsc::sync_channel::<Result<()>>(1);
        let (chunk_tx, chunk_rx) = tokio_mpsc::channel::<Vec<f32>>(64);

        let handle = std::thread::Builder::new()
            .name("lordvarys-mic".into())
            .spawn(move || {
                if let Err(e) = run_stream(chunk_tx, shutdown_rx, &ready_tx) {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .context("spawn mic audio thread")?;

        ready_rx
            .recv()
            .map_err(|_| anyhow!("mic thread died before ready"))??;

        Ok((
            Self {
                shutdown_tx,
                thread: Some(handle),
            },
            chunk_rx,
        ))
    }

    pub fn stop(mut self) {
        let _ = self.shutdown_tx.send(());
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

fn run_stream(
    chunk_tx: tokio_mpsc::Sender<Vec<f32>>,
    shutdown_rx: std_mpsc::Receiver<()>,
    ready_tx: &std_mpsc::SyncSender<Result<()>>,
) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("no default input device"))?;
    let config = device
        .default_input_config()
        .context("default input config")?;

    let source_rate = config.sample_rate().0;
    let channels = config.channels();
    let sample_format = config.sample_format();
    let stream_config = config.into();

    // Ratio used to convert source-rate samples into 16 kHz output. Linear
    // interpolation is plenty for speech-band content, matching soll.
    let ratio = source_rate as f64 / TARGET_SAMPLE_RATE as f64;

    // Per-stream resampler state — cumulative source position and the last
    // sample seen so we can interpolate across callback boundaries.
    let mut accum_source_pos: f64 = 0.0;
    let mut last_sample: f32 = 0.0;
    let mut staged: Vec<f32> = Vec::with_capacity(CHUNK_SAMPLES * 2);

    let err_fn = |err| log::error!("mic audio stream error: {err}");

    let mut handle_input = move |interleaved: &[f32]| {
        let mono = downmix_mono(interleaved, channels);
        for &s in &mono {
            // Walk the source position forward by 1.0 per sample; emit one
            // output sample whenever the integer part of `accum_source_pos`
            // ticks past `ratio`.
            //
            // Equivalent form: we have a virtual output cursor and ask
            // "how many output samples should we emit by the time we've
            // consumed this input sample?" — but the per-sample loop is
            // simpler to read and the overhead is negligible at speech rates.
            let prev = last_sample;
            last_sample = s;
            accum_source_pos += 1.0;

            while accum_source_pos >= ratio {
                let frac = ((accum_source_pos - ratio) / ratio) as f32;
                let interp = prev + (last_sample - prev) * (1.0 - frac.clamp(0.0, 1.0));
                staged.push(interp);
                accum_source_pos -= ratio;
                if staged.len() >= CHUNK_SAMPLES {
                    let chunk = std::mem::replace(&mut staged, Vec::with_capacity(CHUNK_SAMPLES * 2));
                    let _ = chunk_tx.try_send(chunk);
                }
            }
        }
    };

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _| handle_input(data),
            err_fn,
            None,
        )?,
        SampleFormat::I16 => {
            let mut tmp = Vec::<f32>::with_capacity(2048);
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    tmp.clear();
                    tmp.extend(data.iter().map(|&v| v as f32 / i16::MAX as f32));
                    handle_input(&tmp);
                },
                err_fn,
                None,
            )?
        }
        SampleFormat::U16 => {
            let mut tmp = Vec::<f32>::with_capacity(2048);
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    tmp.clear();
                    tmp.extend(data.iter().map(|&v| {
                        (v as f32 - u16::MAX as f32 / 2.0) / (u16::MAX as f32 / 2.0)
                    }));
                    handle_input(&tmp);
                },
                err_fn,
                None,
            )?
        }
        fmt => return Err(anyhow!("unsupported sample format: {fmt:?}")),
    };
    stream.play().context("play stream")?;

    ready_tx
        .send(Ok(()))
        .map_err(|_| anyhow!("mic ready channel dropped"))?;

    let _ = shutdown_rx.recv();
    drop(stream);
    Ok(())
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
