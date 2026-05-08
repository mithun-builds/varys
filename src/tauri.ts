import { invoke } from "@tauri-apps/api/core";

export type PermState = "granted" | "denied" | "unknown";

export interface OnboardingStatus {
  mic_permission: PermState;
  screen_permission: PermState;
  mic_seen: boolean;
  screen_seen: boolean;
  dismissed: boolean;
}

export interface GeneralSettings {
  output_folder: string;
  mic_gain: number;
  sys_gain: number;
}

export interface RecordingStatus {
  is_recording: boolean;
  out_path: string | null;
}

export type TranscriptionState =
  | { kind: "idle" }
  | { kind: "downloading_model"; done_bytes: number; total_bytes: number }
  | { kind: "loading_model" }
  | { kind: "transcribing"; progress_pct: number }
  | { kind: "done"; transcript_path: string }
  | { kind: "failed"; message: string };

export interface RecordingEntry {
  wav_path: string;
  file_name: string;
  has_transcript: boolean;
  transcript_path: string | null;
}

export const tauri = {
  generalSettings: () => invoke<GeneralSettings>("settings_general_get"),
  setOutputFolder: (path: string) =>
    invoke<void>("settings_set_output_folder", { path }),
  setGains: (mic_gain: number, sys_gain: number) =>
    invoke<void>("settings_set_gains", { micGain: mic_gain, sysGain: sys_gain }),
  openOutputFolder: () => invoke<void>("open_output_folder"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  appVersion: () => invoke<string>("app_version"),
  closeSettingsWindow: () => invoke<void>("close_settings_window"),

  recordingStatus: () => invoke<RecordingStatus>("recording_status"),
  startRecording: () => invoke<void>("start_recording"),
  stopRecording: () => invoke<void>("stop_recording"),
  transcriptionStatus: () =>
    invoke<TranscriptionState>("transcription_status"),
  transcribeExisting: (path: string) =>
    invoke<void>("transcribe_existing", { path }),
  listRecordings: () => invoke<RecordingEntry[]>("list_recordings"),
  openPath: (path: string) => invoke<void>("open_path", { path }),

  onboardingStatus: () => invoke<OnboardingStatus>("onboarding_status"),
  onboardingDismiss: () => invoke<void>("onboarding_dismiss"),
  requestMic: () => invoke<void>("request_mic_permission"),
  requestScreenRecording: () =>
    invoke<boolean>("request_screen_recording_permission"),
  openScreenRecordingSettings: () =>
    invoke<void>("open_screen_recording_settings"),
};
