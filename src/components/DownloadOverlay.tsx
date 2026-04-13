/**
 * DownloadOverlay
 * Full-screen overlay shown during first-run model downloads.
 * Minimal progress indicator with step label, bar, and transfer meta.
 */

import type { CSSProperties } from 'react';
import { useTranslation } from 'react-i18next';
import { useRenStore } from '../store';
import { formatBytes, formatSpeed } from '../utils/format';
import styles from './DownloadOverlay.module.css';

const progressPct = (downloaded: number, total: number): number => {
  if (total <= 0) return 0;
  return Math.min(100, Math.max(0, (downloaded / total) * 100));
};

export const DownloadOverlay = () => {
  const { t } = useTranslation();
  const progress = useRenStore((s) => s.downloadProgress);

  if (!progress) return null;

  const pct = progressPct(progress.downloadedBytes, progress.totalBytes);
  const stepLabel = t(`download.steps.${progress.step}`, {
    defaultValue: t('download.unknown_step'),
  });

  const barStyle = { '--progress': `${pct}%` } as CSSProperties;
  const downloaded = formatBytes(progress.downloadedBytes);
  const total = progress.totalBytes > 0 ? formatBytes(progress.totalBytes) : null;
  const speed = formatSpeed(progress.speedBps);

  return (
    <div className={styles.overlay}>
      <span className={styles.title}>{t('welcome.initializing')}</span>
      <span className={styles.step}>{stepLabel}</span>
      <div className={styles.barContainer}>
        <div className={styles.barFill} style={barStyle} />
      </div>
      <span className={styles.meta}>
        {downloaded}
        {total && ` / ${total}`}
        {speed && <span className={styles.separator}>—</span>}
        {speed}
      </span>
    </div>
  );
};
