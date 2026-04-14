/**
 * ClipboardBadge
 * Subtle status pill that surfaces when the user has armed clipboard context
 * via Ctrl+Shift+Alt+V. Shows a short preview and the ESC hint; ESC clears
 * the arm via the `clear_clipboard_arm` Tauri command.
 */

import { useEffect } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { useRenStore } from '../store';
import styles from './ClipboardBadge.module.css';

export const ClipboardBadge = () => {
  const { t } = useTranslation();
  const preview = useRenStore((s) => s.clipboardPreview);

  useEffect(() => {
    if (!preview) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      invoke('clear_clipboard_arm').catch(() => {
        // Backend already emits the cleared event regardless; nothing to do.
      });
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [preview]);

  return (
    <AnimatePresence>
      {preview && (
        <motion.aside
          className={styles.badge}
          initial={{ opacity: 0, y: -8 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -8 }}
          transition={{ duration: 0.2 }}
          aria-live="polite"
        >
          <span className={styles.label}>{t('clipboard.armed_label')}</span>
          <span className={styles.preview} title={preview}>
            {preview}
          </span>
          <span className={styles.hint}>{t('clipboard.armed_hint')}</span>
        </motion.aside>
      )}
    </AnimatePresence>
  );
};
