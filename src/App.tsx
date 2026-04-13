/**
 * Main App Component
 */

import { Orb } from './components/Orb';
import { StateControls } from './components/StateControls';
import { Transcript } from './components/Transcript';
import { DownloadOverlay } from './components/DownloadOverlay';
import { useRenEvents } from './hooks/useRenEvents';
import { useRenStore } from './store';

function App() {
  // Subscribe to all Tauri backend events
  useRenEvents();

  const isVisible = useRenStore((s) => s.isVisible);
  const downloadProgress = useRenStore((s) => s.downloadProgress);

  if (!isVisible) return null;

  // Show download overlay during first-run setup
  if (downloadProgress) return <DownloadOverlay />;

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        position: 'relative',
      }}
    >
      <Orb />
      <Transcript />
      <StateControls />
    </div>
  );
}

export default App;
