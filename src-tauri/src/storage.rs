use chrono::Local;
use std::path::{Path, PathBuf};

/// Build a timestamped output filename for a meeting recording.
///
/// Pattern: `<out_dir>/<YYYY-MM-DD_HH-MM>_<slugified-title>.wav`. If `title` is
/// empty or all-whitespace, the trailing `_<slug>` segment is dropped and the
/// file is named purely by timestamp.
pub fn build_recording_path(out_dir: &Path, title: &str) -> PathBuf {
    let stamp = Local::now().format("%Y-%m-%d_%H-%M").to_string();
    let slug = slugify(title);
    let filename = if slug.is_empty() {
        format!("{stamp}.wav")
    } else {
        format!("{stamp}_{slug}.wav")
    };
    out_dir.join(filename)
}

/// Append a `_mic-only` or `_sys-only` suffix before the `.wav` extension when
/// degraded-mode recordings are saved. Used by recording.rs when one input
/// stream is unavailable.
pub fn with_degraded_suffix(path: &Path, suffix: &str) -> PathBuf {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("recording");
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{stem}_{suffix}.wav"))
}

fn slugify(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut prev_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    // Cap the slug so filenames stay sane on case-folded filesystems.
    if out.len() > 60 {
        out.truncate(60);
        while out.ends_with('-') {
            out.pop();
        }
    }
    out
}

/// Default output folder: a `recordings/` directory at the project root.
///
/// Resolved at compile time via `CARGO_MANIFEST_DIR` (which points at
/// `<repo>/src-tauri`), then walking one level up. Keeps recordings beside
/// the code while iterating in dev. Users can change it via Settings.
///
/// In a future packaged release without the source tree present, fall back
/// to `~/Documents/Lord Varys`.
pub fn default_output_folder() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(repo) = manifest.parent() {
        let candidate = repo.join("recordings");
        if repo.exists() {
            return candidate;
        }
    }
    dirs::document_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("Lord Varys")
}

/// Strings we've used as defaults in past versions. `seed_defaults` migrates
/// any of these to the current default rather than leaving the user stuck on
/// a stale path. Keep this list append-only.
pub fn known_old_defaults() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(docs) = dirs::document_dir() {
        out.push(docs.join("Lord Varys"));
    }
    if let Some(home) = dirs::home_dir() {
        out.push(home.join("Documents").join("Lord Varys"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_works() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("   "), "");
        assert_eq!(slugify("Q4 Roadmap (Draft)"), "q4-roadmap-draft");
        assert_eq!(slugify("---weird---"), "weird");
    }

    #[test]
    fn timestamped_filename_format() {
        let p = build_recording_path(Path::new("/tmp"), "Sync");
        let name = p.file_name().unwrap().to_str().unwrap();
        assert!(name.ends_with("_sync.wav"));
        assert!(name.len() >= "YYYY-MM-DD_HH-MM_sync.wav".len());
    }

    #[test]
    fn empty_title_drops_segment() {
        let p = build_recording_path(Path::new("/tmp"), "   ");
        let name = p.file_name().unwrap().to_str().unwrap();
        assert!(!name.contains("_."));
        assert!(name.ends_with(".wav"));
    }

    #[test]
    fn degraded_suffix() {
        let p = with_degraded_suffix(Path::new("/tmp/2026-05-08_10-30_sync.wav"), "mic-only");
        assert_eq!(
            p.to_str().unwrap(),
            "/tmp/2026-05-08_10-30_sync_mic-only.wav"
        );
    }
}
