/**
 * DownloadOverlay
 * Shown during first-run model downloads.
 * Displays a minimal progress bar with step name and speed.
 */

import { useTranslation } from 'react-i18next';
import { useRenStore } from '../store';
import styles from './DownloadOverlay.module.css';

const formatBytes = (bytes: number): string => {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
};

const formatSpeed = (bps: number): string => {
  if (bps === 0) return '';
  return `${formatBytes(bps)}/s`;
};

export const DownloadOverlay = () => {
  const { t } = useTranslation();
  const progress = useRenStore((s) => s.downloadProgress);

  if (!progress) return null;

  const pct =
    progress.totalBytes > 0
      ? Math.min(100, (progress.downloadedBytes / progress.totalBytes) * 100)
      : 0;

  return (
    <div className={styles.overlay}>
      <span className={styles.title}>{t('welcome.initializing')}</span>
      <span className={styles.step}>{progress.step}</span>
      <div className={styles.barContainer}>
        <div className={styles.barFill} style={{ width: `${pct}%` }} />
      </div>
      <span className={styles.meta}>
        {formatBytes(progress.downloadedBytes)}
        {progress.totalBytes > 0 && ` / ${formatBytes(progress.totalBytes)}`}
        {progress.speedBps > 0 && `  —  ${formatSpeed(progress.speedBps)}`}
      </span>
    </div>
  );
};
