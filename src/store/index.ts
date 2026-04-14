import { create } from 'zustand';
import type { RenState, ErrorState, ToolActivity } from '../types';
import { WAVEFORM_BAR_COUNT } from '../config/ui';

export interface DownloadProgress {
  step: string;
  downloadedBytes: number;
  totalBytes: number;
  speedBps: number;
}

interface RenStore {
  currentState: RenState;
  error: ErrorState | null;
  isVisible: boolean;
  transcript: string | null;
  downloadProgress: DownloadProgress | null;
  waveformAmplitudes: number[];
  toolActivity: ToolActivity | null;
  settingsOpen: boolean;
  clipboardPreview: string | null;

  setState: (state: RenState) => void;
  setError: (error: ErrorState | null) => void;
  toggleVisibility: () => void;
  setVisibility: (visible: boolean) => void;
  setTranscript: (text: string | null) => void;
  setDownloadProgress: (progress: DownloadProgress | null) => void;
  setWaveform: (amplitudes: number[]) => void;
  setToolActivity: (activity: ToolActivity | null) => void;
  setSettingsOpen: (open: boolean) => void;
  setClipboardPreview: (preview: string | null) => void;
}

export const useRenStore = create<RenStore>((set) => ({
  currentState: 'initializing',
  error: null,
  isVisible: true,
  transcript: null,
  downloadProgress: null,
  waveformAmplitudes: Array<number>(WAVEFORM_BAR_COUNT).fill(0),
  toolActivity: null,
  settingsOpen: false,
  clipboardPreview: null,

  setState: (state) => set({ currentState: state, error: null }),

  setError: (error) =>
    set({ currentState: error ? 'error' : 'sleeping', error }),

  toggleVisibility: () => set((s) => ({ isVisible: !s.isVisible })),

  setVisibility: (visible) => set({ isVisible: visible }),

  setTranscript: (text) => set({ transcript: text }),

  setDownloadProgress: (progress) => set({ downloadProgress: progress }),

  setWaveform: (amplitudes) => set({ waveformAmplitudes: amplitudes }),

  setToolActivity: (activity) => set({ toolActivity: activity }),

  setSettingsOpen: (open) => set({ settingsOpen: open }),

  setClipboardPreview: (preview) => set({ clipboardPreview: preview }),
}));
