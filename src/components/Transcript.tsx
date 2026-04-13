/**
 * Transcript Component
 * Displays the last STT transcript below the orb.
 * Fades in on appearance and auto-clears after a timeout (handled in store).
 */

import { AnimatePresence, motion } from 'framer-motion';
import { useRenStore } from '../store';
import styles from './Transcript.module.css';

export const Transcript = () => {
  const transcript = useRenStore((s) => s.transcript);

  return (
    <div className={styles.container}>
      <AnimatePresence>
        {transcript && (
          <motion.p
            className={styles.text}
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.25 }}
          >
            {transcript}
          </motion.p>
        )}
      </AnimatePresence>
    </div>
  );
};
