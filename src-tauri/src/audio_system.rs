//! System-audio capture by spawning the `sckit_capture` Swift sidecar.
//!
//! The sidecar writes a JSON header line on stdout describing the stream
//! (sample rate, channels, format), then raw interleaved f32 PCM bytes until
//! killed. We read the header, then funnel chunks of decoded f32 samples
//! through a tokio mpsc channel.
//!
//! The sidecar binary is bundled via `tauri.conf.json::bundle.externalBin`,
//! resolved at runtime through `tauri::path::resolve_resource`. In dev mode
//! we also try `src-tauri/binaries/sckit_capture-aarch64-apple-darwin` and
//! `target/release/sckit_capture` as fallbacks so `pnpm tauri:dev` works
//! before a full bundle is produced.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Stdio;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

#[derive(Deserialize, Debug, Clone)]
pub struct StreamHeader {
    pub sample_rate: u32,
    pub channels: u16,
    pub format: String, // "f32"
}

pub struct SystemAudioRecorder {
    child: Child,
    pub header: StreamHeader,
}

impl SystemAudioRecorder {
    /// Spawn the sidecar in capture mode and read the header. Returns the
    /// recorder + a receiver yielding raw interleaved samples (de-interleaved
    /// downstream by the mixer).
    pub async fn start(app: &AppHandle) -> Result<(Self, mpsc::Receiver<Vec<f32>>)> {
        let bin = resolve_sidecar_path(app)?;
        log::info!("spawning sckit_capture: {}", bin.display());

        let mut child = Command::new(&bin)
            .arg("--format")
            .arg("pcm-f32")
            .arg("--sample-rate")
            .arg("48000")
            .arg("--channels")
            .arg("2")
            .arg("--exclude-self")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawn sckit_capture at {}", bin.display()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("sckit_capture stdout missing"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("sckit_capture stderr missing"))?;

        // Spawn a stderr drainer so the sidecar never blocks on a full pipe.
        // Each line gets logged at info level; the sidecar treats stderr as
        // its log channel.
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                log::info!("sckit_capture: {line}");
            }
        });

        let mut reader = BufReader::new(stdout);
        let mut header_line = String::new();
        reader
            .read_line(&mut header_line)
            .await
            .context("read sckit_capture header")?;
        let header: StreamHeader = serde_json::from_str(header_line.trim())
            .with_context(|| format!("parse header: {header_line:?}"))?;
        if header.format != "f32" {
            return Err(anyhow!(
                "unsupported sidecar format: {} (expected f32)",
                header.format
            ));
        }

        let (chunk_tx, chunk_rx) = mpsc::channel::<Vec<f32>>(64);

        // 4 KiB chunks of raw bytes → 1024 f32 samples per send. The sidecar
        // writes ~10 ms of audio per write at 48 kHz stereo, so this matches
        // the sidecar's natural cadence without coalescing.
        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            let mut leftover: Vec<u8> = Vec::with_capacity(4);
            let mut inner = reader.into_inner();
            loop {
                let n = match inner.read(&mut buf).await {
                    Ok(0) => break, // EOF — sidecar exited
                    Ok(n) => n,
                    Err(e) => {
                        log::warn!("sckit_capture read error: {e}");
                        break;
                    }
                };
                let mut bytes: Vec<u8> = leftover.split_off(0);
                bytes.extend_from_slice(&buf[..n]);

                // f32 LE = 4 bytes; carry over any partial sample.
                let usable = (bytes.len() / 4) * 4;
                let (full, partial) = bytes.split_at(usable);
                leftover.extend_from_slice(partial);

                let samples: Vec<f32> = full
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                if !samples.is_empty() && chunk_tx.send(samples).await.is_err() {
                    break;
                }
            }
        });

        Ok((Self { child, header }, chunk_rx))
    }

    pub async fn stop(mut self) {
        // Try graceful termination first; fall back to kill if the sidecar
        // doesn't honour SIGTERM within a couple hundred ms.
        let _ = self.child.start_kill();
        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            self.child.wait(),
        )
        .await;
    }
}

/// Probe Screen Recording permission by spawning the sidecar in `--probe`
/// mode. macOS shows the consent dialog on the first invocation against a
/// given code-signed bundle. We fire-and-forget — the dialog appears
/// asynchronously, the user clicks Allow/Deny, and the next call to
/// `CGPreflightScreenCaptureAccess` (polled by the React UI every 2 s)
/// picks up the new state. Waiting for the probe to exit was both racy and
/// caused fake "timeout" errors when the user was slow on the prompt.
pub fn probe_permission(app: &AppHandle) -> Result<()> {
    let bin = resolve_sidecar_path(app)?;
    log::info!("probing screen recording via {}", bin.display());
    Command::new(&bin)
        .arg("--probe")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawn probe at {}", bin.display()))?;
    Ok(())
}

fn resolve_sidecar_path(app: &AppHandle) -> Result<PathBuf> {
    // Production: Tauri places externalBin sidecars next to the main binary
    // in Contents/MacOS/, NOT in Contents/Resources/. Look there first.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            for name in ["sckit_capture", "sckit_capture-aarch64-apple-darwin", "sckit_capture-x86_64-apple-darwin"] {
                let p = parent.join(name);
                if p.exists() {
                    return Ok(p);
                }
            }
        }
    }

    // Tauri's resource resolver — covers any case where bundle layout shifts
    // in a future Tauri version. Cheap to check.
    if let Ok(p) = app
        .path()
        .resolve("sckit_capture", tauri::path::BaseDirectory::Resource)
    {
        if p.exists() {
            return Ok(p);
        }
    }

    resolve_sidecar_path_dev_fallback()
}

fn resolve_sidecar_path_dev_fallback() -> Result<PathBuf> {
    // CARGO_MANIFEST_DIR is reliable across `tauri dev` cwd weirdness.
    #[cfg(debug_assertions)]
    {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for rel in [
            "binaries/sckit_capture-aarch64-apple-darwin",
            "binaries/sckit_capture-x86_64-apple-darwin",
            "binaries/sckit_capture",
            "swift-helper/.build/release/sckit_capture",
            "swift-helper/.build/debug/sckit_capture",
        ] {
            let p = manifest.join(rel);
            if p.exists() {
                return Ok(p);
            }
        }
    }

    let candidates = [
        "src-tauri/binaries/sckit_capture-aarch64-apple-darwin",
        "src-tauri/binaries/sckit_capture-x86_64-apple-darwin",
        "src-tauri/binaries/sckit_capture",
        "src-tauri/swift-helper/.build/release/sckit_capture",
        "src-tauri/swift-helper/.build/debug/sckit_capture",
    ];
    let cwd = std::env::current_dir().context("cwd")?;
    for rel in candidates {
        let p = cwd.join(rel);
        if p.exists() {
            return Ok(p);
        }
        if let Some(parent) = cwd.parent() {
            let p2 = parent.join(rel);
            if p2.exists() {
                return Ok(p2);
            }
        }
    }
    Err(anyhow!(
        "sckit_capture sidecar not found. Build it via `cd src-tauri/swift-helper && swift build -c release`."
    ))
}
