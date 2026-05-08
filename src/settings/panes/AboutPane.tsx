import { useEffect, useState } from "react";
import { tauri } from "../../tauri";

export function AboutPane() {
  const [version, setVersion] = useState("…");

  useEffect(() => {
    tauri.appVersion().then(setVersion).catch(() => setVersion("?"));
  }, []);

  return (
    <>
      <h2>About</h2>
      <div className="row">
        <label>Version</label>
        <span className="code">{version}</span>
      </div>
      <div className="row">
        <label>Status</label>
        <span className="muted">Milestone 1 — capture only (no transcription yet)</span>
      </div>
      <div className="muted help" style={{ marginTop: 24, lineHeight: 1.6 }}>
        Lord Varys is an ambient memory layer for work. Detect meeting → silently
        capture system + microphone audio → save WAV. Transcription, summaries,
        and semantic search land in future milestones.
      </div>
    </>
  );
}
