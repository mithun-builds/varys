import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  tauri,
  GeneralSettings,
  RecordingStatus,
  TranscriptionState,
  RecordingEntry,
} from "../../tauri";

export function GeneralPane() {
  const [s, setS] = useState<GeneralSettings | null>(null);
  const [rec, setRec] = useState<RecordingStatus>({
    is_recording: false,
    out_path: null,
  });
  const [trState, setTrState] = useState<TranscriptionState>({ kind: "idle" });
  const [recordings, setRecordings] = useState<RecordingEntry[]>([]);
  const [busy, setBusy] = useState(false);

  const refresh = async () => {
    try {
      const [gs, rs, ts, list] = await Promise.all([
        tauri.generalSettings(),
        tauri.recordingStatus(),
        tauri.transcriptionStatus(),
        tauri.listRecordings(),
      ]);
      setS(gs);
      setRec(rs);
      setTrState(ts);
      setRecordings(list);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 1500);
    return () => clearInterval(id);
  }, []);

  if (!s) return <div className="pane-loading">Loading…</div>;

  const pickFolder = async () => {
    const picked = await openDialog({
      directory: true,
      multiple: false,
      defaultPath: s.output_folder,
    });
    if (typeof picked === "string") {
      await tauri.setOutputFolder(picked);
      setS({ ...s, output_folder: picked });
    }
  };

  const updateGain = async (key: "mic_gain" | "sys_gain", value: number) => {
    const next = { ...s, [key]: value };
    setS(next);
    try {
      await tauri.setGains(next.mic_gain, next.sys_gain);
    } catch (e) {
      console.error(e);
    }
  };

  const toggle = async () => {
    setBusy(true);
    try {
      if (rec.is_recording) {
        await tauri.stopRecording();
      } else {
        await tauri.startRecording();
      }
      await refresh();
    } catch (e) {
      console.error(e);
      alert(`Recording error: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const transcribing =
    trState.kind === "downloading_model" ||
    trState.kind === "loading_model" ||
    trState.kind === "transcribing";

  return (
    <>
      <h1>General</h1>

      <div className="pane-section">
        <h2>Recording</h2>
        <div className="record-card">
          <button
            type="button"
            className={`record-btn ${rec.is_recording ? "on" : ""}`}
            onClick={toggle}
            disabled={busy}
          >
            <span className="record-dot" />
            {busy ? "…" : rec.is_recording ? "Stop Recording" : "Start Recording"}
          </button>
          <div className="record-meta">
            {rec.is_recording
              ? "Capturing mic + system audio"
              : transcribing
              ? "Idle (transcription running)"
              : "Idle"}
          </div>
        </div>
      </div>

      <div className="pane-section">
        <h2>Transcription</h2>
        <TranscriptionStatus state={trState} onCancel={() => tauri.cancelTranscription().then(refresh)} />
      </div>

      <div className="pane-section">
        <h2>Recordings</h2>
        {recordings.length === 0 ? (
          <div className="empty-hint">No recordings yet. Click Start Recording above.</div>
        ) : (
          <ul className="row-list">
            {recordings.slice(0, 10).map((r) => (
              <li className="row" key={r.wav_path}>
                <div className="row-main">
                  <div className="row-title">{r.file_name}</div>
                  <div className="row-hint subtle">
                    {r.has_transcript ? "Transcribed" : "Not transcribed"}
                  </div>
                </div>
                <button className="secondary" onClick={() => tauri.openPath(r.wav_path)}>
                  Audio
                </button>
                {r.has_transcript && r.transcript_path ? (
                  <button className="secondary" onClick={() => tauri.openPath(r.transcript_path!)}>
                    Transcript
                  </button>
                ) : (
                  <button
                    className="primary"
                    onClick={() => tauri.transcribeExisting(r.wav_path).then(refresh)}
                    disabled={transcribing}
                  >
                    Transcribe
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="pane-section">
        <h2>Output folder</h2>
        <div className="folder-row">
          <code className="folder-path">{s.output_folder}</code>
          <button className="secondary" onClick={pickFolder}>Choose…</button>
          <button className="secondary" onClick={() => tauri.openOutputFolder()}>Reveal</button>
        </div>
        <div className="hint subtle">
          Each recording saves a stereo <code>.wav</code> (L=mic, R=system) plus a
          markdown <code>.txt</code> and structured <code>.json</code> after
          transcription.
        </div>
      </div>

      <div className="pane-section">
        <h2>Mix levels</h2>
        <div className="gain-row">
          <label className="gain-label">Microphone</label>
          <input
            type="range"
            min={0}
            max={2}
            step={0.05}
            value={s.mic_gain}
            onChange={(e) => updateGain("mic_gain", parseFloat(e.target.value))}
          />
          <code className="gain-value">{s.mic_gain.toFixed(2)}</code>
        </div>
        <div className="gain-row">
          <label className="gain-label">System</label>
          <input
            type="range"
            min={0}
            max={2}
            step={0.05}
            value={s.sys_gain}
            onChange={(e) => updateGain("sys_gain", parseFloat(e.target.value))}
          />
          <code className="gain-value">{s.sys_gain.toFixed(2)}</code>
        </div>
      </div>
    </>
  );
}

function TranscriptionStatus({
  state,
  onCancel,
}: {
  state: TranscriptionState;
  onCancel: () => void;
}) {
  switch (state.kind) {
    case "idle":
      return (
        <div className="empty-hint">
          No transcription running. Stop a recording to start one automatically.
        </div>
      );
    case "downloading_model": {
      const pct =
        state.total_bytes > 0
          ? Math.floor((state.done_bytes * 100) / state.total_bytes)
          : 0;
      const mb = (n: number) => (n / 1024 / 1024).toFixed(0);
      return (
        <div className="status-card">
          <div className="status-line">
            <span className="status-label">Downloading model</span>
            <span className="status-detail">
              {mb(state.done_bytes)} / {mb(state.total_bytes)} MB ({pct}%)
            </span>
          </div>
          <ProgressBar pct={pct} />
        </div>
      );
    }
    case "loading_model":
      return (
        <div className="status-card">
          <div className="status-line">
            <span className="status-label">Loading model</span>
            <span className="status-detail">Initialising Metal…</span>
          </div>
        </div>
      );
    case "transcribing":
      return (
        <div className="status-card">
          <div className="status-line">
            <span className="status-label">Transcribing</span>
            <span className="status-detail">Whispering… {state.progress_pct}%</span>
            <button className="secondary" onClick={onCancel}>Cancel</button>
          </div>
          <ProgressBar pct={state.progress_pct} />
        </div>
      );
    case "done":
      return (
        <div className="status-card status-card--ok">
          <div className="status-line">
            <span className="status-label">Done</span>
            <code className="status-detail-mono">{state.transcript_path}</code>
            <button className="secondary" onClick={() => tauri.openPath(state.transcript_path)}>
              Open transcript
            </button>
          </div>
        </div>
      );
    case "cancelled":
      return (
        <div className="status-card">
          <div className="status-line">
            <span className="status-label">Cancelled</span>
            <span className="status-detail subtle">Transcription was aborted.</span>
          </div>
        </div>
      );
    case "failed":
      return (
        <div className="pane-error">
          <strong>Transcription failed:</strong> {state.message}
        </div>
      );
  }
}

function ProgressBar({ pct }: { pct: number }) {
  return (
    <div className="progress-track">
      <div className="progress-fill" style={{ width: `${Math.min(100, Math.max(0, pct))}%` }} />
    </div>
  );
}
