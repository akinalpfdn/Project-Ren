/**
 * Main App Component
 * Renders the core Ren orb interface
 */

import { useEffect } from 'react';
import { Orb } from './components/Orb';
import { StateControls } from './components/StateControls';
import { useRenStore } from './store';

function App() {
  const { setState, isVisible } = useRenStore();

  useEffect(() => {
    // Simulate initialization sequence
    const timer = setTimeout(() => {
      setState('sleeping');
    }, 2000);

    return () => clearTimeout(timer);
  }, [setState]);

  if (!isVisible) {
    return null;
  }

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
      <StateControls />
    </div>
  );
}

export default App;
