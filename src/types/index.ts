/**
 * Core type definitions
 */

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
