/**
 * Orb Component
 * Core visual element that animates based on Ren's state
 */

import { motion, AnimatePresence } from 'framer-motion';
import { useRenStore } from '../store';
import styles from './Orb.module.css';

export const Orb = () => {
  const currentState = useRenStore((state) => state.currentState);

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
            {Array.from({ length: 8 }).map((_, i) => (
              <div key={i} className={styles.waveBar} />
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
        transition={{
          duration: 0.5,
          ease: 'easeOut',
        }}
      >
        <AnimatePresence mode="wait">{renderStateVisual()}</AnimatePresence>
      </motion.div>
    </div>
  );
};
