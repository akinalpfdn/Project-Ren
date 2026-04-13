/**
 * Zustand Store
 * Mock state machine for Phase 1 - real state machine will be in Rust backend
 */

import { create } from 'zustand';
import type { RenState, ErrorState } from '../types';

interface RenStore {
  // Current state
  currentState: RenState;

  // Error state
  error: ErrorState | null;

  // Window visibility
  isVisible: boolean;

  // Actions (mock for Phase 1)
  setState: (state: RenState) => void;
  setError: (error: ErrorState | null) => void;
  toggleVisibility: () => void;
  setVisibility: (visible: boolean) => void;
}

export const useRenStore = create<RenStore>((set) => ({
  currentState: 'initializing',
  error: null,
  isVisible: true,

  setState: (state) => set({ currentState: state, error: null }),

  setError: (error) =>
    set({
      currentState: error ? 'error' : 'sleeping',
      error,
    }),

  toggleVisibility: () => set((state) => ({ isVisible: !state.isVisible })),

  setVisibility: (visible) => set({ isVisible: visible }),
}));
