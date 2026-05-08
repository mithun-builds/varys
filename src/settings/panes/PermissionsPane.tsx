import { useEffect, useState } from "react";
import { tauri, OnboardingStatus, PermState } from "../../tauri";

function badgeFor(state: PermState) {
  switch (state) {
    case "granted":
      return <span className="badge ok">Granted</span>;
    case "denied":
      return <span className="badge err">Denied</span>;
    default:
      return <span className="badge warn">Not yet asked</span>;
  }
}

export function PermissionsPane() {
  const [status, setStatus] = useState<OnboardingStatus | null>(null);
  const [probing, setProbing] = useState(false);

  const refresh = async () => {
    try {
      setStatus(await tauri.onboardingStatus());
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 2000);
    return () => clearInterval(id);
  }, []);

  if (!status) return <div className="muted">Loading…</div>;

  const probeScreen = async () => {
    setProbing(true);
    try {
      await tauri.requestScreenRecording();
    } finally {
      setProbing(false);
      refresh();
    }
  };

  return (
    <>
      <h2>Permissions</h2>
      <div className="muted help" style={{ marginBottom: 16 }}>
        Lord Varys needs both microphone and screen recording access to capture
        the full audio mix. macOS shows the prompts the first time each
        subsystem is invoked.
      </div>

      <div className="perm-row">
        <div className="perm-meta">
          <span className="perm-title">Microphone</span>
          <span className="perm-detail">Captures your voice during recordings.</span>
        </div>
        <div>
          {badgeFor(status.mic_permission)}
          <button className="btn" onClick={() => tauri.requestMic().then(refresh)}>
            Request
          </button>
        </div>
      </div>

      <div className="perm-row">
        <div className="perm-meta">
          <span className="perm-title">Screen Recording</span>
          <span className="perm-detail">
            Required by ScreenCaptureKit for capturing system/speaker audio.
            Video frames are never stored.
          </span>
        </div>
        <div>
          {badgeFor(status.screen_permission)}
          <button className="btn" onClick={probeScreen} disabled={probing}>
            {probing ? "Probing…" : "Request"}
          </button>
          <button
            className="btn"
            onClick={() => tauri.openScreenRecordingSettings()}
            style={{ marginLeft: 6 }}
          >
            Open Settings
          </button>
        </div>
      </div>
    </>
  );
}
