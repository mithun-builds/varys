import { useEffect, useState } from "react";
import { tauri } from "../../tauri";

export function AboutPane() {
  const [version, setVersion] = useState("…");

  useEffect(() => {
    tauri.appVersion().then(setVersion).catch(() => setVersion("?"));
  }, []);

  return (
    <>
      <h1>About</h1>

      <div className="pane-section">
        <div className="row-list">
          <div className="row">
            <div className="row-main">
              <div className="row-title">Version</div>
              <div className="row-hint subtle">Lord Varys v{version}</div>
            </div>
          </div>
          <div className="row">
            <div className="row-main">
              <div className="row-title">Source</div>
              <div className="row-hint subtle">github.com/mithun-builds/varys</div>
            </div>
            <button
              className="secondary"
              onClick={() => tauri.openUrl("https://github.com/mithun-builds/varys")}
            >
              Open
            </button>
          </div>
          <div className="row">
            <div className="row-main">
              <div className="row-title">Update via Homebrew</div>
              <div className="row-hint subtle">
                <code>brew upgrade --cask varys</code>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="hint subtle" style={{ lineHeight: 1.6 }}>
        Lord Varys is an ambient memory layer for work — silently captures
        microphone and system audio when you click Start, transcribes locally
        with whisper.cpp, and writes timestamped transcripts beside each WAV.
        Nothing leaves your machine.
      </div>
    </>
  );
}
