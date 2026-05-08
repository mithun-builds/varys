import { useEffect, useRef, useState } from "react";
import {
  tauri,
  OnboardingStatus,
  ModelInfo,
  TranscriptionState,
  RecordingStatus,
} from "../tauri";
import { VarysLogo } from "../components/Logo";

type StepState = "done" | "in_progress" | "denied" | "pending";

interface StepDef {
  id: string;
  iconNode: React.ReactNode;
  title: string;
  state: StepState;
  desc: string;
  onToggleOn?: () => void | Promise<void>;
  onToggleOff?: () => void | Promise<void>;
  onNote?: string;
  extra?: React.ReactNode;
}

// ── Icons (kept simple — match soll's stroke style) ────────────────────────

const ICONS: Record<string, React.ReactNode> = {
  mic: (
    <svg className="ob-step-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
      <rect x="9" y="2" width="6" height="12" rx="3" />
      <path d="M5 11a7 7 0 0 0 14 0" />
      <line x1="12" y1="18" x2="12" y2="22" />
      <line x1="8" y1="22" x2="16" y2="22" />
    </svg>
  ),
  screen: (
    <svg className="ob-step-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
      <rect x="2.5" y="4" width="19" height="13" rx="2" />
      <line x1="8" y1="21" x2="16" y2="21" />
      <line x1="12" y1="17" x2="12" y2="21" />
      <circle cx="12" cy="10.5" r="2.5" />
    </svg>
  ),
  model: (
    <svg className="ob-step-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round">
      <rect x="1" y="10" width="2" height="4" rx="1" />
      <rect x="5" y="7" width="2" height="10" rx="1" />
      <rect x="9" y="3" width="2" height="18" rx="1" />
      <rect x="13" y="3" width="2" height="18" rx="1" />
      <rect x="17" y="7" width="2" height="10" rx="1" />
      <rect x="21" y="10" width="2" height="4" rx="1" />
    </svg>
  ),
  record: (
    <svg className="ob-step-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="4" fill="currentColor" stroke="none" />
    </svg>
  ),
};

function cssState(state: StepState) {
  return state === "in_progress" ? "in-progress" : state;
}

function StatusBadge({ state }: { state: StepState }) {
  const label: Record<StepState, string> = {
    done: "Done ✓",
    in_progress: "In Progress…",
    denied: "Access Denied",
    pending: "Pending",
  };
  return <span className={`ob-badge ob-badge--${cssState(state)}`}>{label[state]}</span>;
}

function Toggle({
  on,
  disabled,
  onEnable,
  onDisable,
}: {
  on: boolean;
  disabled?: boolean;
  onEnable?: () => void;
  onDisable?: () => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      disabled={disabled}
      className={`ob-toggle ${on ? "ob-toggle--on" : ""}`}
      onClick={() => (on ? onDisable?.() : onEnable?.())}
    />
  );
}

function ModelPicker({
  models,
  downloadingId,
  downloadProgress,
  onPick,
}: {
  models: ModelInfo[];
  downloadingId: string | null;
  downloadProgress: { done_bytes: number; total_bytes: number } | null;
  onPick: (m: ModelInfo) => void;
}) {
  return (
    <div className="ob-model-picker">
      {models.map((m) => {
        const isDownloading = downloadingId === m.id;
        const otherIsDownloading = downloadingId !== null && !isDownloading;
        const on = (m.is_cached && m.is_active) || isDownloading;
        const disabled = (m.is_cached && m.is_active) || otherIsDownloading;
        const pct =
          downloadProgress && isDownloading && downloadProgress.total_bytes > 0
            ? Math.floor((downloadProgress.done_bytes * 100) / downloadProgress.total_bytes)
            : 0;
        const status = isDownloading
          ? `Downloading… ${pct}%`
          : m.is_cached && !m.is_active
          ? "Cached · click to use"
          : !m.is_cached
          ? "Not downloaded"
          : null;
        return (
          <button
            key={m.id}
            type="button"
            className={`ob-model-card${on ? " ob-model-card--on" : ""}`}
            disabled={disabled}
            onClick={() => onPick(m)}
          >
            <div className="ob-model-card-info">
              <div className="ob-model-card-name">{m.short_name}</div>
              <div className="ob-model-card-size">{m.size_label}</div>
              {m.id === "small.en" && (
                <div className="ob-model-card-rec">★ Recommended</div>
              )}
              {status && <div className="ob-model-card-pulled">{status}</div>}
            </div>
            <span className={`ob-model-card-toggle${on ? " ob-model-card-toggle--on" : ""}`} />
          </button>
        );
      })}
    </div>
  );
}

function RecordTest({
  status,
  rec,
  onToggle,
}: {
  status: OnboardingStatus;
  rec: RecordingStatus;
  onToggle: () => void;
}) {
  const blockers: string[] = [];
  if (status.mic_permission !== "granted")
    blockers.push("Microphone access not granted — see Step 1.");
  if (status.screen_permission !== "granted")
    blockers.push("Screen Recording access not granted — see Step 2.");
  if (!status.model_cached)
    blockers.push("Whisper model not downloaded — see Step 3.");

  const ready = blockers.length === 0;

  return (
    <div className="ob-dictation-test">
      {!ready ? (
        <div className="ob-dictation-blockers">
          <strong>Blocking issues</strong>
          <ul>{blockers.map((b, i) => <li key={i}>{b}</li>)}</ul>
        </div>
      ) : (
        <>
          <button
            type="button"
            className={`record-btn ${rec.is_recording ? "on" : ""}`}
            onClick={onToggle}
          >
            <span className="record-dot" />
            {rec.is_recording ? "Stop Recording" : "Start a test recording"}
          </button>
          <p className="ob-dictation-hint">
            Speak for ~10 seconds, then click Stop. Lord Varys will save the
            WAV and transcribe it automatically.
          </p>
          {status.has_recorded && (
            <p className="ob-toggle-note">
              ✓ At least one recording captured. You're set up.
            </p>
          )}
        </>
      )}
    </div>
  );
}

// ── Step derivation ────────────────────────────────────────────────────────

interface DeriveOpts {
  models: ModelInfo[];
  downloadingId: string | null;
  downloadProgress: { done_bytes: number; total_bytes: number } | null;
  onPickModel: (m: ModelInfo) => void;
  rec: RecordingStatus;
  onToggleRecording: () => void;
}

function deriveSteps(s: OnboardingStatus, opts: DeriveOpts): StepDef[] {
  const micState: StepState =
    s.mic_permission === "granted"
      ? "done"
      : s.mic_permission === "denied"
      ? "denied"
      : "pending";

  const screenState: StepState =
    s.screen_permission === "granted"
      ? "done"
      : s.screen_permission === "denied"
      ? "denied"
      : "pending";

  const modelState: StepState = s.model_cached
    ? "done"
    : opts.downloadingId
    ? "in_progress"
    : "pending";

  const recState: StepState = s.has_recorded ? "done" : "pending";

  // ── Step 1: Microphone ──────────────────────────────────────────────────
  const micStep: StepDef = {
    id: "mic",
    iconNode: ICONS.mic,
    title: "Microphone access",
    state: micState,
    desc:
      micState === "denied"
        ? "Microphone access was previously declined. Toggle on to open System Settings — macOS won't show the dialog a second time."
        : "Lord Varys captures your microphone during recordings to record what you say.",
    onToggleOn:
      micState === "pending"
        ? () => tauri.requestMic()
        : micState === "denied"
        ? () => tauri.openPrivacySettings("Privacy_Microphone")
        : undefined,
    onToggleOff:
      micState === "done"
        ? () => tauri.openPrivacySettings("Privacy_Microphone")
        : undefined,
    onNote:
      micState === "done"
        ? "macOS only allows revoking via System Settings — toggling off opens the right pane."
        : undefined,
  };

  // ── Step 2: Screen Recording ────────────────────────────────────────────
  // Granting screen recording requires a restart for the new TCC bit to take
  // effect on the running process — surface a Restart action when pending.
  const screenStep: StepDef = {
    id: "screen",
    iconNode: ICONS.screen,
    title: "Screen Recording access",
    state: screenState,
    desc:
      screenState === "denied"
        ? "Screen Recording was previously declined. Toggle on to open System Settings."
        : "Required by ScreenCaptureKit to capture system audio (the other meeting participants). Video frames are never stored — only audio is read from the SCKit stream.",
    onToggleOn:
      screenState === "pending"
        ? async () => {
            await tauri.requestScreenRecording();
          }
        : screenState === "denied"
        ? () => tauri.openPrivacySettings("Privacy_ScreenCapture")
        : undefined,
    onToggleOff:
      screenState === "done"
        ? () => tauri.openPrivacySettings("Privacy_ScreenCapture")
        : undefined,
    onNote:
      screenState === "done"
        ? "macOS only allows revoking via System Settings — toggling off opens the right pane."
        : undefined,
    extra:
      screenState !== "done" ? (
        <div className="ob-step-extra-stack">
          <p className="ob-toggle-note">
            Already granted in System Settings? macOS caches the permission until
            the app restarts.
          </p>
          <button
            type="button"
            className="ob-action-btn"
            onClick={() => tauri.restartApp()}
          >
            Restart Lord Varys to apply
          </button>
        </div>
      ) : undefined,
  };

  // ── Step 3: Whisper model ───────────────────────────────────────────────
  const activeModel = opts.models.find((m) => m.is_active);
  const focusModel = activeModel ?? opts.models.find((m) => m.is_cached) ?? opts.models[0];
  const focusLabel = focusModel ? `${focusModel.short_name} (${focusModel.size_label})` : "Small (466 MB)";
  const modelStep: StepDef = {
    id: "model",
    iconNode: ICONS.model,
    title: "Speech recognition model",
    state: modelState,
    desc: opts.downloadingId
      ? `Downloading ${focusLabel}…`
      : modelState === "done"
      ? `${focusLabel} is ready. You can switch models anytime from Settings.`
      : "Pick the model you want — toggle one on to download. You can change this later in Settings.",
    extra: (
      <ModelPicker
        models={opts.models}
        downloadingId={opts.downloadingId}
        downloadProgress={opts.downloadProgress}
        onPick={opts.onPickModel}
      />
    ),
  };

  // ── Step 4: First recording ─────────────────────────────────────────────
  const recStep: StepDef = {
    id: "record",
    iconNode: ICONS.record,
    title: "Try a recording",
    state: recState,
    desc:
      recState === "done"
        ? "You've captured at least one recording. You can keep practising, or finish the setup."
        : "Click Start, speak (and play a video for system-audio testing), then Stop. Lord Varys will save the WAV and auto-transcribe.",
    extra: <RecordTest status={s} rec={opts.rec} onToggle={opts.onToggleRecording} />,
  };

  return [micStep, screenStep, modelStep, recStep];
}

// ── Dot progress + nav ─────────────────────────────────────────────────────

function StepDots({ steps, current, onDotClick }: { steps: StepDef[]; current: number; onDotClick: (i: number) => void }) {
  return (
    <div className="ob-dots">
      {steps.map((s, i) => (
        <button
          key={s.id}
          type="button"
          className={i === current ? "ob-dot ob-dot--active" : s.state === "done" ? "ob-dot ob-dot--done" : "ob-dot"}
          onClick={() => onDotClick(i)}
          title={s.title}
        />
      ))}
    </div>
  );
}

function WizardStep({ step, index, total, animDir }: { step: StepDef; index: number; total: number; animDir: "right" | "left" }) {
  const hasHandlers = !!step.onToggleOn || !!step.onToggleOff;
  const showToggle = hasHandlers;
  const toggleOn = step.state === "done" || step.state === "in_progress";
  const wantedHandler = toggleOn ? step.onToggleOff : step.onToggleOn;
  const toggleDisabled = !wantedHandler;

  return (
    <div className={`ob-slide ob-slide--enter-${animDir}`}>
      <div className="ob-slide-inner">
        <div className="ob-step-icon-wrap">{step.iconNode}</div>
        <div className="ob-step-meta">
          <span className="ob-step-num">Step {index + 1} of {total}</span>
          <StatusBadge state={step.state} />
        </div>
        <div className="ob-step-title">{step.title}</div>
        <p className="ob-step-desc">{step.desc}</p>
        {showToggle && (
          <Toggle on={toggleOn} disabled={toggleDisabled} onEnable={step.onToggleOn} onDisable={step.onToggleOff} />
        )}
        {step.onNote && toggleOn && <p className="ob-toggle-note">{step.onNote}</p>}
        {step.extra && <div className="ob-step-extra">{step.extra}</div>}
      </div>
    </div>
  );
}

// ── Main ───────────────────────────────────────────────────────────────────

export function OnboardingApp() {
  const [status, setStatus] = useState<OnboardingStatus | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [trState, setTrState] = useState<TranscriptionState>({ kind: "idle" });
  const [rec, setRec] = useState<RecordingStatus>({ is_recording: false, out_path: null });
  const [currentStep, setCurrentStep] = useState(0);
  const [animDir, setAnimDir] = useState<"right" | "left">("right");
  const [animKey, setAnimKey] = useState(0);
  const [visited, setVisited] = useState<Set<number>>(() => new Set([0]));
  const polling = useRef(false);

  async function fetchStatus() {
    if (polling.current) return;
    polling.current = true;
    try {
      const [s, m, ts, rs] = await Promise.all([
        tauri.onboardingStatus(),
        tauri.listModels(),
        tauri.transcriptionStatus(),
        tauri.recordingStatus(),
      ]);
      setStatus(s);
      setModels(m);
      setTrState(ts);
      setRec(rs);
    } catch (err) {
      console.error("onboarding fetch failed:", err);
    } finally {
      polling.current = false;
    }
  }

  useEffect(() => {
    void fetchStatus();
    const id = setInterval(() => void fetchStatus(), 1500);
    return () => clearInterval(id);
  }, []);

  function goTo(next: number) {
    if (next === currentStep) return;
    setAnimDir(next > currentStep ? "right" : "left");
    setCurrentStep(next);
    setAnimKey((k) => k + 1);
    setVisited((prev) => {
      if (prev.has(next)) return prev;
      const out = new Set(prev);
      out.add(next);
      return out;
    });
  }

  async function pickModel(m: ModelInfo) {
    if (m.is_active && m.is_cached) return;
    await tauri.setWhisperModel(m.id);
    if (!m.is_cached) {
      await tauri.downloadModel(m.id);
    }
    fetchStatus();
  }

  async function toggleRecording() {
    try {
      if (rec.is_recording) {
        await tauri.stopRecording();
      } else {
        await tauri.startRecording();
      }
      fetchStatus();
    } catch (e) {
      console.error(e);
      alert(`Recording error: ${e}`);
    }
  }

  async function completeAndDismiss() {
    try {
      await tauri.onboardingDismiss();
    } finally {
      await tauri.closeOnboardingWindow();
    }
  }

  async function closeWithoutDismissing() {
    const ok = window.confirm(
      "Setup is incomplete.\n\n" +
      "Some steps are still pending — without them, recording or transcription " +
      "may not work. You can reopen this guide anytime from the Lord Varys icon " +
      "in the menu bar.\n\n" +
      "Close anyway?"
    );
    if (!ok) return;
    await tauri.closeOnboardingWindow();
  }

  if (!status) {
    return (
      <div className="ob-shell">
        <div className="ob-loading">Loading setup guide…</div>
      </div>
    );
  }

  const downloadingId = trState.kind === "downloading_model" ? trState.model : null;
  const downloadProgress =
    trState.kind === "downloading_model"
      ? { done_bytes: trState.done_bytes, total_bytes: trState.total_bytes }
      : null;

  const steps = deriveSteps(status, {
    models,
    downloadingId,
    downloadProgress,
    onPickModel: (m) => void pickModel(m),
    rec,
    onToggleRecording: () => void toggleRecording(),
  });

  const doneCount = steps.filter((s, i) => s.state === "done" && visited.has(i)).length;
  const allDone = steps.every((s) => s.state === "done");
  const pct = Math.round((doneCount / steps.length) * 100);
  const isFirst = currentStep === 0;
  const isLast = currentStep === steps.length - 1;

  return (
    <div className="ob-shell">
      <div className="ob-header">
        <VarysLogo className="ob-logo" />
        <div>
          <div className="ob-title">Welcome to Lord Varys</div>
          <div className="ob-subtitle">
            Let's get you set up. Complete the steps below to start recording.
          </div>
        </div>
      </div>

      <div className="ob-progress-wrap">
        <div className="ob-progress-bar">
          <div className="ob-progress-fill" style={{ width: `${pct}%` }} />
        </div>
        <span className="ob-progress-label">
          {doneCount}/{steps.length} steps
        </span>
      </div>

      <WizardStep key={animKey} step={steps[currentStep]} index={currentStep} total={steps.length} animDir={animDir} />

      <div className="ob-nav">
        <button type="button" className="ob-nav-btn" onClick={() => goTo(currentStep - 1)} disabled={isFirst}>
          ← Back
        </button>
        <StepDots steps={steps} current={currentStep} onDotClick={goTo} />
        {isLast ? (
          allDone ? (
            <button type="button" className="ob-nav-btn ob-nav-btn--primary" onClick={() => void completeAndDismiss()}>
              All Done ✓
            </button>
          ) : (
            <button type="button" className="ob-nav-btn" onClick={() => void closeWithoutDismissing()}>
              Close
            </button>
          )
        ) : (
          <button type="button" className="ob-nav-btn ob-nav-btn--next" onClick={() => goTo(currentStep + 1)}>
            Next →
          </button>
        )}
      </div>
    </div>
  );
}

