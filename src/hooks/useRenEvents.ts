/**
 * useRenEvents
 * Subscribes to all Tauri backend events and syncs them into the Zustand store.
 * Call once at the app root.
 */

import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useRenStore } from '../store';
import type {
  StateChangedPayload,
  TranscriptPayload,
  DownloadProgressPayload,
  ErrorPayload,
  WaveformPayload,
} from '../types';

export const useRenEvents = () => {
  const { setState, setError, setTranscript, setDownloadProgress, setWaveform } =
    useRenStore();

  useEffect(() => {
    const unlisten: Array<() => void> = [];

    const setup = async () => {
      unlisten.push(
        await listen<StateChangedPayload>('ren://state-changed', (event) => {
          setState(event.payload.state);
        })
      );

      unlisten.push(
        await listen<TranscriptPayload>('ren://transcript', (event) => {
          if (event.payload.is_final) {
            setTranscript(event.payload.text);
            // Auto-clear transcript after 8 seconds
            setTimeout(() => setTranscript(null), 8000);
          }
        })
      );

      unlisten.push(
        await listen<DownloadProgressPayload>(
          'ren://download-progress',
          (event) => {
            const p = event.payload;
            setDownloadProgress({
              step: p.step,
              downloadedBytes: p.downloaded_bytes,
              totalBytes: p.total_bytes,
              speedBps: p.speed_bps,
            });
          }
        )
      );

      unlisten.push(
        await listen<ErrorPayload>('ren://error', (event) => {
          setError({
            code: event.payload.code,
            message: event.payload.message,
            timestamp: Date.now(),
          });
        })
      );

      unlisten.push(
        await listen<WaveformPayload>('ren://waveform', (event) => {
          setWaveform(event.payload.amplitudes);
        })
      );
    };

    setup();

    return () => {
      unlisten.forEach((fn) => fn());
    };
  }, [setState, setError, setTranscript, setDownloadProgress, setWaveform]);
};
