/**
 * Zustand Store
 * Mock state machine for Phase 1 - real state machine will be in Rust backend
 */

import { create } from 'zustand';
import type { RenState, ErrorState } from '../types';

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

  setState: (state: RenState) => void;
  setError: (error: ErrorState | null) => void;
  toggleVisibility: () => void;
  setVisibility: (visible: boolean) => void;
  setTranscript: (text: string | null) => void;
  setDownloadProgress: (progress: DownloadProgress | null) => void;
}

export const useRenStore = create<RenStore>((set) => ({
  currentState: 'initializing',
  error: null,
  isVisible: true,
  transcript: null,
  downloadProgress: null,

  setState: (state) => set({ currentState: state, error: null }),

  setError: (error) =>
    set({ currentState: error ? 'error' : 'sleeping', error }),

  toggleVisibility: () => set((s) => ({ isVisible: !s.isVisible })),

  setVisibility: (visible) => set({ isVisible: visible }),

  setTranscript: (text) => set({ transcript: text }),

  setDownloadProgress: (progress) => set({ downloadProgress: progress }),
}));
