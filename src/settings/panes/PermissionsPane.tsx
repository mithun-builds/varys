import { useEffect, useState } from "react";
import { tauri, OnboardingStatus, PermState } from "../../tauri";

function badgeFor(state: PermState) {
  switch (state) {
    case "granted":
      return <span className="ob-badge ob-badge--done">Granted</span>;
    case "denied":
      return <span className="ob-badge ob-badge--denied">Denied</span>;
    default:
      return <span className="ob-badge ob-badge--pending">Not yet asked</span>;
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

  if (!status) return <div className="pane-loading">Loading…</div>;

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
      <h1>Permissions</h1>
      <div className="hint subtle" style={{ marginTop: -8 }}>
        Lord Varys needs both microphone and screen recording access to capture
        the full audio mix. macOS shows the prompts the first time each
        subsystem is invoked.
      </div>

      <div className="row-list">
        <div className="row column">
          <div className="row-clickable" style={{ cursor: "default" }}>
            <div className="row-main">
              <div className="row-title">Microphone</div>
              <div className="row-hint subtle">
                Captures your voice during recordings.
              </div>
            </div>
            {badgeFor(status.mic_permission)}
            <button className="secondary" onClick={() => tauri.requestMic().then(refresh)}>
              Request
            </button>
            <button
              className="secondary"
              onClick={() => tauri.openPrivacySettings("Privacy_Microphone")}
            >
              System Settings
            </button>
          </div>
        </div>

        <div className="row column">
          <div className="row-clickable" style={{ cursor: "default" }}>
            <div className="row-main">
              <div className="row-title">Screen Recording</div>
              <div className="row-hint subtle">
                Required by ScreenCaptureKit for capturing system / speaker audio.
                Video frames are never stored.
              </div>
            </div>
            {badgeFor(status.screen_permission)}
            <button className="secondary" onClick={probeScreen} disabled={probing}>
              {probing ? "Probing…" : "Request"}
            </button>
            <button
              className="secondary"
              onClick={() => tauri.openPrivacySettings("Privacy_ScreenCapture")}
            >
              System Settings
            </button>
          </div>
        </div>
      </div>

      <div className="hint subtle">
        Screen Recording requires a restart after granting. Use the menu bar's
        Quit option, then relaunch.
      </div>
    </>
  );
}
