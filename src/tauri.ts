import { invoke } from "@tauri-apps/api/core";

export type PermState = "granted" | "denied" | "unknown";

export interface OnboardingStatus {
  mic_permission: PermState;
  screen_permission: PermState;
  mic_seen: boolean;
  screen_seen: boolean;
  model_cached: boolean;
  has_recorded: boolean;
  dismissed: boolean;
}

export interface GeneralSettings {
  output_folder: string;
  mic_gain: number;
  sys_gain: number;
  whisper_model: string;
}

export interface RecordingStatus {
  is_recording: boolean;
  out_path: string | null;
}

export type TranscriptionState =
  | { kind: "idle" }
  | { kind: "downloading_model"; model: string; done_bytes: number; total_bytes: number }
  | { kind: "loading_model" }
  | { kind: "transcribing"; progress_pct: number }
  | { kind: "cancelled" }
  | { kind: "done"; transcript_path: string }
  | { kind: "failed"; message: string };

export interface RecordingEntry {
  wav_path: string;
  file_name: string;
  has_transcript: boolean;
  transcript_path: string | null;
}

export interface ModelInfo {
  id: string;
  short_name: string;
  size_label: string;
  is_cached: boolean;
  is_active: boolean;
}

export const tauri = {
  generalSettings: () => invoke<GeneralSettings>("settings_general_get"),
  setOutputFolder: (path: string) =>
    invoke<void>("settings_set_output_folder", { path }),
  setGains: (mic_gain: number, sys_gain: number) =>
    invoke<void>("settings_set_gains", { micGain: mic_gain, sysGain: sys_gain }),
  setWhisperModel: (model: string) =>
    invoke<void>("settings_set_whisper_model", { model }),
  openOutputFolder: () => invoke<void>("open_output_folder"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  openPrivacySettings: (section: string) =>
    invoke<void>("open_privacy_settings", { section }),
  appVersion: () => invoke<string>("app_version"),
  restartApp: () => invoke<void>("restart_app"),
  closeSettingsWindow: () => invoke<void>("close_settings_window"),
  closeOnboardingWindow: () => invoke<void>("close_onboarding_window"),

  recordingStatus: () => invoke<RecordingStatus>("recording_status"),
  startRecording: () => invoke<void>("start_recording"),
  stopRecording: () => invoke<void>("stop_recording"),
  transcriptionStatus: () => invoke<TranscriptionState>("transcription_status"),
  cancelTranscription: () => invoke<void>("cancel_transcription"),
  transcribeExisting: (path: string) =>
    invoke<void>("transcribe_existing", { path }),
  listRecordings: () => invoke<RecordingEntry[]>("list_recordings"),

  listModels: () => invoke<ModelInfo[]>("list_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),

  onboardingStatus: () => invoke<OnboardingStatus>("onboarding_status"),
  onboardingDismiss: () => invoke<void>("onboarding_dismiss"),
  requestMic: () => invoke<void>("request_mic_permission"),
  requestScreenRecording: () =>
    invoke<boolean>("request_screen_recording_permission"),
  openScreenRecordingSettings: () =>
    invoke<void>("open_screen_recording_settings"),
};
