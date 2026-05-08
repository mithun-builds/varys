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
  const [saving, setSaving] = useState(false);
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

  if (!s) return <div className="muted">Loading…</div>;

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
    setSaving(true);
    try {
      await tauri.setGains(next.mic_gain, next.sys_gain);
    } finally {
      setSaving(false);
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

  return (
    <>
      <h2>General</h2>

      <h3>Recording</h3>
      <div className="row">
        <button
          className={`btn ${rec.is_recording ? "" : "btn-primary"}`}
          onClick={toggle}
          disabled={busy}
        >
          {busy ? "…" : rec.is_recording ? "■ Stop Recording" : "● Start Recording"}
        </button>
        <span className="muted" style={{ marginLeft: 12 }}>
          {rec.is_recording ? "Capturing mic + system audio…" : "Idle"}
        </span>
      </div>
      {rec.out_path && (
        <div className="help">
          Output: <span className="code">{rec.out_path}</span>
        </div>
      )}

      <h3>Transcription</h3>
      <TranscriptionStatus state={trState} />

      <h3>Recordings</h3>
      {recordings.length === 0 ? (
        <div className="muted">No recordings yet.</div>
      ) : (
        <div>
          {recordings.map((r) => (
            <div className="perm-row" key={r.wav_path}>
              <div className="perm-meta">
                <span className="perm-title">{r.file_name}</span>
                <span className="perm-detail">
                  {r.has_transcript ? "Transcribed" : "Not transcribed"}
                </span>
              </div>
              <div>
                <button
                  className="btn"
                  onClick={() => tauri.openPath(r.wav_path)}
                  title="Open the WAV in your default audio app"
                >
                  Audio
                </button>
                {r.has_transcript && r.transcript_path ? (
                  <button
                    className="btn"
                    style={{ marginLeft: 6 }}
                    onClick={() => tauri.openPath(r.transcript_path!)}
                  >
                    Transcript
                  </button>
                ) : (
                  <button
                    className="btn btn-primary"
                    style={{ marginLeft: 6 }}
                    onClick={() =>
                      tauri.transcribeExisting(r.wav_path).then(refresh)
                    }
                    disabled={trState.kind !== "idle" && trState.kind !== "done" && trState.kind !== "failed"}
                  >
                    Transcribe
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}

      <h3>Output folder</h3>
      <div className="row">
        <input
          type="text"
          value={s.output_folder}
          readOnly
          className="input"
        />
        <button className="btn" onClick={pickFolder}>
          Choose…
        </button>
        <button className="btn" onClick={() => tauri.openOutputFolder()}>
          Reveal
        </button>
      </div>
      <div className="help">
        Recording saves <span className="code">.wav</span> (stereo: L=mic,
        R=system) plus, after transcription, <span className="code">.txt</span>{" "}
        and <span className="code">.json</span>.
      </div>

      <h3>Mix levels</h3>
      <div className="row">
        <label>Microphone gain</label>
        <input
          type="range"
          min={0}
          max={2}
          step={0.05}
          value={s.mic_gain}
          onChange={(e) => updateGain("mic_gain", parseFloat(e.target.value))}
          className="slider"
        />
        <span className="code">{s.mic_gain.toFixed(2)}</span>
      </div>
      <div className="row">
        <label>System gain</label>
        <input
          type="range"
          min={0}
          max={2}
          step={0.05}
          value={s.sys_gain}
          onChange={(e) => updateGain("sys_gain", parseFloat(e.target.value))}
          className="slider"
        />
        <span className="code">{s.sys_gain.toFixed(2)}</span>
      </div>
      {saving && <div className="muted help">Saving…</div>}
    </>
  );
}

function TranscriptionStatus({ state }: { state: TranscriptionState }) {
  switch (state.kind) {
    case "idle":
      return (
        <div className="muted">
          No transcription running. Stop a recording to start one.
        </div>
      );
    case "downloading_model": {
      const pct =
        state.total_bytes > 0
          ? Math.floor((state.done_bytes * 100) / state.total_bytes)
          : 0;
      const mb = (n: number) => (n / 1024 / 1024).toFixed(0);
      return (
        <div>
          <div>
            <span className="badge warn">Downloading model</span>
            <span style={{ marginLeft: 8 }}>
              small.en — {mb(state.done_bytes)} / {mb(state.total_bytes)} MB ({pct}%)
            </span>
          </div>
          <ProgressBar pct={pct} />
        </div>
      );
    }
    case "loading_model":
      return (
        <div>
          <span className="badge warn">Loading model</span>
          <span style={{ marginLeft: 8 }}>Initialising Metal…</span>
        </div>
      );
    case "transcribing":
      return (
        <div>
          <div>
            <span className="badge warn">Transcribing</span>
            <span style={{ marginLeft: 8 }}>
              Whispering… {state.progress_pct}%
            </span>
          </div>
          <ProgressBar pct={state.progress_pct} />
        </div>
      );
    case "done":
      return (
        <div>
          <span className="badge ok">Done</span>
          <span style={{ marginLeft: 8 }}>
            <span className="code">{state.transcript_path}</span>
          </span>
          <button
            className="btn"
            style={{ marginLeft: 10 }}
            onClick={() => tauri.openPath(state.transcript_path)}
          >
            Open transcript
          </button>
        </div>
      );
    case "failed":
      return (
        <div>
          <span className="badge err">Failed</span>
          <span style={{ marginLeft: 8 }} className="muted">
            {state.message}
          </span>
        </div>
      );
  }
}

function ProgressBar({ pct }: { pct: number }) {
  return (
    <div
      style={{
        height: 4,
        background: "var(--bg-input)",
        borderRadius: 2,
        marginTop: 6,
        overflow: "hidden",
      }}
    >
      <div
        style={{
          width: `${Math.min(100, Math.max(0, pct))}%`,
          height: "100%",
          background: "var(--accent)",
          transition: "width 200ms ease",
        }}
      />
    </div>
  );
}
