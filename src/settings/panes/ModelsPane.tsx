import { useEffect, useState } from "react";
import { tauri, ModelInfo, TranscriptionState } from "../../tauri";

export function ModelsPane() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [trState, setTrState] = useState<TranscriptionState>({ kind: "idle" });

  const refresh = async () => {
    try {
      const [m, ts] = await Promise.all([
        tauri.listModels(),
        tauri.transcriptionStatus(),
      ]);
      setModels(m);
      setTrState(ts);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 1500);
    return () => clearInterval(id);
  }, []);

  const downloading = trState.kind === "downloading_model" ? trState : null;

  const handleClick = async (m: ModelInfo) => {
    if (downloading) return;
    if (m.is_active && m.is_cached) return;
    await tauri.setWhisperModel(m.id);
    if (!m.is_cached) {
      await tauri.downloadModel(m.id);
    }
    refresh();
  };

  return (
    <>
      <h1>Whisper model</h1>
      <div className="hint subtle" style={{ marginTop: -8 }}>
        Local speech-to-text engine. Smaller is faster; larger is more accurate.
        Switch any time — the new model is used on the next transcription.
      </div>

      <div className="model-grid">
        {models.map((m) => (
          <ModelCard
            key={m.id}
            model={m}
            downloadingId={downloading?.model ?? null}
            downloadProgress={
              downloading && downloading.model === m.id
                ? downloading
                : null
            }
            onClick={() => handleClick(m)}
          />
        ))}
      </div>
    </>
  );
}

function ModelCard({
  model,
  downloadingId,
  downloadProgress,
  onClick,
}: {
  model: ModelInfo;
  downloadingId: string | null;
  downloadProgress: { done_bytes: number; total_bytes: number } | null;
  onClick: () => void;
}) {
  const isDownloading = downloadingId === model.id;
  const otherIsDownloading = downloadingId !== null && !isDownloading;
  const on = (model.is_cached && model.is_active) || isDownloading;
  const disabled = (model.is_cached && model.is_active) || otherIsDownloading;

  const pct =
    downloadProgress && downloadProgress.total_bytes > 0
      ? Math.floor((downloadProgress.done_bytes * 100) / downloadProgress.total_bytes)
      : 0;

  const statusLine = isDownloading
    ? `Downloading… ${pct}%`
    : model.is_cached && !model.is_active
    ? "Cached · click to use"
    : !model.is_cached
    ? "Not downloaded"
    : null;

  return (
    <button
      type="button"
      className={`ob-model-card${on ? " ob-model-card--on" : ""}`}
      disabled={disabled}
      onClick={onClick}
    >
      <div className="ob-model-card-info">
        <div className="ob-model-card-name">{model.short_name}</div>
        <div className="ob-model-card-size">{model.size_label}</div>
        {model.id === "small.en" && (
          <div className="ob-model-card-rec">★ Recommended</div>
        )}
        {statusLine && <div className="ob-model-card-pulled">{statusLine}</div>}
      </div>
      <span className={`ob-model-card-toggle${on ? " ob-model-card-toggle--on" : ""}`} />
    </button>
  );
}
