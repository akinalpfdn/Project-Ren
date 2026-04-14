/**
 * App
 * Root component. Subscribes to backend events and composes the main surface.
 */

import { Orb } from './components/Orb';
import { Transcript } from './components/Transcript';
import { ToolCard } from './components/ToolCard';
import { DownloadOverlay } from './components/DownloadOverlay';
import { Settings } from './components/Settings';
import { ClipboardBadge } from './components/ClipboardBadge';
import { useRenEvents } from './hooks/useRenEvents';
import { useRenStore } from './store';
import styles from './App.module.css';

const App = () => {
  useRenEvents();

  const isVisible = useRenStore((s) => s.isVisible);
  const downloadProgress = useRenStore((s) => s.downloadProgress);

  if (!isVisible) return null;
  if (downloadProgress) return <DownloadOverlay />;

  return (
    <main className={styles.stage}>
      <ClipboardBadge />
      <ToolCard />
      <Orb />
      <Transcript />
      <Settings />
    </main>
  );
};

export default App;
