/**
 * Orb
 * Core visual element. Modifier class selects state appearance;
 * speaking state renders bars whose height is driven by backend RMS amplitudes.
 */

import { motion } from 'framer-motion';
import { useRenStore } from '../store';
import type { RenState } from '../types';
import { WAVEFORM_BAR_COUNT } from '../config/ui';
import styles from './Orb.module.css';

const PARTICLE_COUNT = 4;
const WAVEFORM_MIN_SCALE = 0.15;

const StateVisual = ({
  state,
  amplitudes,
}: {
  state: RenState;
  amplitudes: number[];
}) => {
  if (state === 'thinking') {
    return (
      <div className={styles.particles}>
        {Array.from({ length: PARTICLE_COUNT }, (_, i) => (
          <div key={i} className={styles.particle} />
        ))}
      </div>
    );
  }

  if (state === 'speaking') {
    return (
      <div className={styles.waveform}>
        {Array.from({ length: WAVEFORM_BAR_COUNT }, (_, i) => {
          const amp = amplitudes[i] ?? 0;
          const scale = Math.max(WAVEFORM_MIN_SCALE, amp);
          return (
            <div
              key={i}
              className={styles.waveBar}
              style={{ transform: `scaleY(${scale})` }}
            />
          );
        })}
      </div>
    );
  }

  return null;
};

export const Orb = () => {
  const currentState = useRenStore((s) => s.currentState);
  const waveformAmplitudes = useRenStore((s) => s.waveformAmplitudes);

  return (
    <div className={styles.container}>
      <motion.div
        data-tauri-drag-region
        className={`${styles.orb} ${styles[currentState]}`}
        initial={{ scale: 0, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        transition={{ duration: 0.5, ease: 'easeOut' }}
      >
        <StateVisual state={currentState} amplitudes={waveformAmplitudes} />
      </motion.div>
    </div>
  );
};
