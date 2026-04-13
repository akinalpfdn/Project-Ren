/**
 * Orb Component
 * Core visual element that animates based on Ren's state.
 * Speaking state uses real waveform data from the TTS playback.
 */

import { motion, AnimatePresence } from 'framer-motion';
import { useRenStore } from '../store';
import styles from './Orb.module.css';

export const Orb = () => {
  const currentState = useRenStore((s) => s.currentState);
  const waveformAmplitudes = useRenStore((s) => s.waveformAmplitudes);

  const renderStateVisual = () => {
    switch (currentState) {
      case 'thinking':
        return (
          <div className={styles.particles}>
            <div className={styles.particle} />
            <div className={styles.particle} />
            <div className={styles.particle} />
            <div className={styles.particle} />
          </div>
        );

      case 'speaking':
        return (
          <div className={styles.waveform}>
            {waveformAmplitudes.map((amp, i) => (
              <div
                key={i}
                className={styles.waveBar}
                style={{
                  // Data-driven height: 15% minimum, up to 100%
                  transform: `scaleY(${Math.max(0.15, amp)})`,
                }}
              />
            ))}
          </div>
        );

      default:
        return null;
    }
  };

  return (
    <div className={styles.container}>
      <motion.div
        className={`${styles.orb} ${styles[currentState]}`}
        initial={{ scale: 0, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        transition={{ duration: 0.5, ease: 'easeOut' }}
      >
        <AnimatePresence mode="wait">{renderStateVisual()}</AnimatePresence>
      </motion.div>
    </div>
  );
};
