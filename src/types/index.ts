export type RenState =
  | 'initializing'
  | 'sleeping'
  | 'waking'
  | 'listening'
  | 'thinking'
  | 'speaking'
  | 'idle'
  | 'error';

export interface StateTransition {
  from: RenState;
  to: RenState;
  timestamp: number;
}

export interface ErrorState {
  message: string;
  code?: string;
  timestamp: number;
}

// Tauri event payloads (mirrors Rust structs)
export interface StateChangedPayload {
  state: RenState;
}

export interface TranscriptPayload {
  text: string;
  is_final: boolean;
}

export interface DownloadProgressPayload {
  step: string;
  downloaded_bytes: number;
  total_bytes: number;
  speed_bps: number;
}

export interface ErrorPayload {
  code: string;
  message: string;
}
