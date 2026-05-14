import { invoke } from "@tauri-apps/api/core";

export interface Settings {
  groq_key: string;
  kb_folder: string;
  nudge_enabled: boolean;
  ai_suggestions_enabled: boolean;
  nudge_interval_secs: number;
  nudge_threshold: number;
  whisper_model: string;
  screen_share_protection: boolean;
  auto_start: boolean;
  obsidian_vault: string;
  webhook_url: string;
  parakeet_enabled: boolean;
  mcp_enabled: boolean;
}

export interface KbSearchResult {
  chunk_id: number;
  file_path: string;
  breadcrumb: string;
  snippet: string;
  score: number;
}

export interface DeviceInfo {
  name: string;
  kind: "input" | "monitor";
}

export interface WhisperStatus {
  ready: boolean;
  model_name: string;
  model_path: string;
}

export interface TranscriptSegment {
  meeting_id: string;
  source: "you" | "speaker";
  text: string;
  start_ms: number;
  end_ms: number;
  is_final: boolean;
}

export const ipc = {
  // Settings
  settingsGet: (): Promise<Settings> => invoke("settings_get"),
  settingsSet: (key: keyof Settings, value: string): Promise<void> =>
    invoke("settings_set", { key, value }),
  groqTestConnection: (key: string): Promise<boolean> =>
    invoke("groq_test_connection", { key }),

  // KB
  kbIndexStart: (folder: string): Promise<void> =>
    invoke("kb_index_start", { folder }),
  kbReindexAll: (): Promise<void> => invoke("kb_reindex_all"),
  kbSearch: (query: string, topK?: number): Promise<KbSearchResult[]> =>
    invoke("kb_search", { query, topK }),

  // Audio / Whisper
  audioDevicesList: (): Promise<DeviceInfo[]> =>
    invoke("audio_devices_list"),
  whisperModelStatus: (): Promise<WhisperStatus> =>
    invoke("whisper_model_status"),
  whisperDownloadModel: (modelName: string): Promise<void> =>
    invoke("whisper_download_model", { modelName }),

  // Meeting
  meetingStart: (title: string, platform?: string): Promise<string> =>
    invoke("meeting_start", { title, platform: platform ?? null }),
  meetingStop: (): Promise<string> => invoke("meeting_stop"),

  // Library
  meetingsList: (): Promise<MeetingRow[]> => invoke("meetings_list"),
  meetingSearch: (query: string): Promise<MeetingRow[]> =>
    invoke("meeting_search", { query }),
  chatQuery: (question: string): Promise<ChatResponse> =>
    invoke("chat_query", { question }),

  // Post-meeting
  // Telemetry
  logFilePath: (): Promise<string> => invoke("log_file_path"),

  // Auto-detect
  autoStartEnable: (): Promise<void> => invoke("auto_start_enable"),
  autoStartDisable: (): Promise<void> => invoke("auto_start_disable"),

  // Overlay
  overlayShow: (): Promise<void> => invoke("overlay_show"),
  overlayHide: (): Promise<void> => invoke("overlay_hide"),
  overlayToggle: (): Promise<void> => invoke("overlay_toggle"),

  meetingGet: (id: string): Promise<MeetingDetail> => invoke("meeting_get", { id }),
  actionItemToggle: (id: number, done: boolean): Promise<void> =>
    invoke("action_item_toggle", { id, done }),
  meetingNotesSave: (id: string, notes: string): Promise<void> =>
    invoke("meeting_notes_save", { id, notes }),
  meetingExportMarkdown: (id: string): Promise<string> =>
    invoke("meeting_export_markdown", { id }),
  meetingRegenerateSummary: (id: string): Promise<void> =>
    invoke("meeting_regenerate_summary", { id }),
};

export interface MeetingRow {
  id: string;
  title: string;
  platform: string | null;
  status: "recording" | "processing" | "done" | "error";
  started_at: number; // unix ms
  ended_at: number | null;
  duration_ms: number | null;
  segment_count: number;
}

export interface SourceMeeting {
  id: string;
  title: string;
  started_at: number;
}

export interface ChatResponse {
  answer: string;
  sources: SourceMeeting[];
}

export interface SummaryData {
  overview: string;
  decisions: string[];
  topics: string[];
}

export interface ActionItemData {
  id: number;
  text: string;
  assignee: string | null;
  due_date: string | null;
  done: boolean;
}

export interface SegmentData {
  source: string;
  speaker_name: string | null;
  text: string;
  start_ms: number;
  end_ms: number;
}

export interface JobData {
  kind: string;
  status: string;
  error: string | null;
}

export interface MeetingDetail {
  id: string;
  title: string;
  status: string;
  started_at: number;
  ended_at: number | null;
  duration_ms: number | null;
  notes: string | null;
  summary: SummaryData | null;
  action_items: ActionItemData[];
  segments: SegmentData[];
  jobs: JobData[];
}
