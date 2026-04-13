/**
 * ToolCard Component
 * Shows the currently-running tool and its most recent result just above
 * the orb. Fades in on tool-executing events and out once a tool-result
 * event arrives plus a short dwell window.
 */

import { AnimatePresence, motion } from 'framer-motion';
import { useTranslation } from 'react-i18next';
import { useRenStore } from '../store';
import type { ToolActivityStatus } from '../types';
import styles from './ToolCard.module.css';

const STATUS_CLASS: Record<ToolActivityStatus, string> = {
  running: styles.running,
  success: styles.success,
  failure: styles.failure,
};

export const ToolCard = () => {
  const activity = useRenStore((s) => s.toolActivity);
  const { t } = useTranslation();

  return (
    <div className={styles.container}>
      <AnimatePresence>
        {activity && (
          <motion.div
            key={`${activity.tool}-${activity.startedAt}`}
            className={styles.card}
            role="status"
            aria-live="polite"
            initial={{ opacity: 0, y: -6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.25 }}
          >
            <span
              className={`${styles.indicator} ${STATUS_CLASS[activity.status]}`}
              aria-hidden="true"
            />
            <div className={styles.body}>
              <span className={styles.label}>
                {t(`tools.status.${activity.status}`)} ·{' '}
                {t(`tools.names.${activity.tool}`, { defaultValue: t('tools.fallback_name') })}
              </span>
              <span className={styles.message}>{activity.message}</span>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
};
