/**
 * State Controls Component (Mock for Phase 1 only)
 * Temporary UI to test state transitions - will be removed in later phases
 */

import { useRenStore } from '../store';
import type { RenState } from '../types';
import styles from './StateControls.module.css';

const STATES: RenState[] = [
  'sleeping',
  'waking',
  'listening',
  'thinking',
  'speaking',
  'idle',
];

export const StateControls = () => {
  const { currentState, setState } = useRenStore();

  return (
    <div className={styles.container}>
      <span className={styles.label}>Mock State Controls (Phase 1)</span>
      <div style={{ display: 'flex', gap: 'var(--ren-space-2)' }}>
        {STATES.map((state) => (
          <button
            key={state}
            className={`${styles.button} ${
              currentState === state ? styles.active : ''
            }`}
            onClick={() => setState(state)}
          >
            {state}
          </button>
        ))}
      </div>
    </div>
  );
};
