//! Whisper model catalog + downloader.
//!
//! Three models on offer — pick one in Settings → General → Whisper model.
//! Downloaded on first use, cached forever in `$APP_DATA/models/`.

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use log::info;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhisperModel {
    TinyEn,
    SmallEn,
    MediumEn,
}

impl WhisperModel {
    pub const DEFAULT: Self = Self::SmallEn;

    pub fn from_id(s: &str) -> Option<Self> {
        match s {
            "tiny.en" | "tiny_en" => Some(Self::TinyEn),
            "small.en" | "small_en" => Some(Self::SmallEn),
            "medium.en" | "medium_en" => Some(Self::MediumEn),
            _ => None,
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            Self::TinyEn => "tiny.en",
            Self::SmallEn => "small.en",
            Self::MediumEn => "medium.en",
        }
    }

    pub fn filename(&self) -> String {
        format!("ggml-{}.bin", self.id())
    }

    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            self.filename()
        )
    }

    pub fn expected_size_bytes(&self) -> u64 {
        match self {
            Self::TinyEn => 39 * 1024 * 1024,
            Self::SmallEn => 466 * 1024 * 1024,
            Self::MediumEn => 1_420 * 1024 * 1024,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::TinyEn => "Tiny — fastest (39 MB, ~3x faster, lower accuracy)",
            Self::SmallEn => "Small — balanced (466 MB, default)",
            Self::MediumEn => "Medium — most accurate (1.4 GB, ~2x slower)",
        }
    }
}

pub fn model_dir(app: &AppHandle) -> Result<PathBuf> {
    let base = app.path().app_data_dir().context("app data dir")?;
    Ok(base.join("models"))
}

pub fn model_path(app: &AppHandle, model: WhisperModel) -> Result<PathBuf> {
    Ok(model_dir(app)?.join(model.filename()))
}

pub fn is_cached(app: &AppHandle, model: WhisperModel) -> bool {
    let Ok(p) = model_path(app, model) else { return false };
    let Ok(meta) = std::fs::metadata(&p) else {
        return false;
    };
    meta.len() >= model.expected_size_bytes() * 9 / 10
}

/// Download `model` if not cached. Calls `on_progress` while bytes stream in.
/// Resumes from a `.part` file if a previous download was interrupted.
pub async fn ensure_model(
    app: &AppHandle,
    model: WhisperModel,
    mut on_progress: impl FnMut(u64, u64) + Send,
) -> Result<PathBuf> {
    let dir = model_dir(app)?;
    tokio::fs::create_dir_all(&dir).await.ok();
    let path = dir.join(model.filename());
    let min_size = model.expected_size_bytes() * 9 / 10;

    if let Ok(meta) = tokio::fs::metadata(&path).await {
        if meta.len() >= min_size {
            return Ok(path);
        }
        let _ = tokio::fs::remove_file(&path).await;
    }

    let tmp = path.with_extension("bin.part");
    let mut resume_from: u64 = match tokio::fs::metadata(&tmp).await {
        Ok(m) => m.len(),
        Err(_) => 0,
    };

    info!(
        "downloading {} from {} -> {} (resume_from={resume_from})",
        model.id(),
        model.url(),
        path.display()
    );
    let client = reqwest::Client::builder().build().context("http client")?;
    let mut req = client.get(model.url());
    if resume_from > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
    }
    let resp = req.send().await.context("model download request")?;
    let status = resp.status();
    let supports_resume = status == reqwest::StatusCode::PARTIAL_CONTENT;
    if !supports_resume && resume_from > 0 {
        info!("server ignored Range; restarting model download");
        let _ = tokio::fs::remove_file(&tmp).await;
        resume_from = 0;
    }
    if !status.is_success() {
        return Err(anyhow!("download status {status}"));
    }
    let total = resume_from + resp.content_length().unwrap_or(0);

    let mut file = if resume_from > 0 {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&tmp)
            .await
            .context("open .part for append")?
    } else {
        tokio::fs::File::create(&tmp).await.context("create .part")?
    };
    let mut stream = resp.bytes_stream();
    let mut done: u64 = resume_from;
    on_progress(done, total);
    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("download chunk")?;
        file.write_all(&chunk).await.context("write chunk")?;
        done += chunk.len() as u64;
        on_progress(done, total);
    }
    file.flush().await.ok();
    drop(file);
    tokio::fs::rename(&tmp, &path).await.context("rename .part → .bin")?;
    info!("downloaded {} ({} bytes)", model.id(), done);
    Ok(path)
}
