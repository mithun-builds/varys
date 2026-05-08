//! Point whisper.cpp at the bundled Metal shader so GPU acceleration actually
//! initialises. Lifted verbatim from soll/src-tauri/src/metal.rs.

#[cfg(target_os = "macos")]
pub fn ensure_metal_resources() {
    if std::env::var_os("GGML_METAL_PATH_RESOURCES").is_some() {
        return;
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(contents_dir) = exe.parent().and_then(|p| p.parent()) {
            let resources = contents_dir.join("Resources");
            let shader = resources.join("ggml-metal.metal");
            if shader.exists() {
                std::env::set_var("GGML_METAL_PATH_RESOURCES", &resources);
                log::info!("metal: bundle resources = {}", resources.display());
                return;
            }
        }
    }

    #[cfg(debug_assertions)]
    {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let resources = std::path::Path::new(manifest).join("resources");
        let shader = resources.join("ggml-metal.metal");
        if shader.exists() {
            std::env::set_var("GGML_METAL_PATH_RESOURCES", &resources);
            log::info!("metal: dev resources = {}", resources.display());
            return;
        }
    }

    log::warn!("metal: ggml-metal.metal not found; whisper falls back to CPU");
}

#[cfg(not(target_os = "macos"))]
pub fn ensure_metal_resources() {}
