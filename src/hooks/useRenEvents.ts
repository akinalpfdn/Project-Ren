/**
 * useRenEvents
 * Subscribes to all Tauri backend events and syncs them into the Zustand store.
 * Call once at the app root.
 */

import { useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useRenStore } from '../store';
import { TRANSCRIPT_VISIBLE_MS, TOOL_CARD_VISIBLE_MS } from '../config/ui';
import type {
  StateChangedPayload,
  TranscriptPayload,
  DownloadProgressPayload,
  ErrorPayload,
  WaveformPayload,
  ToolExecutingPayload,
  ToolResultPayload,
} from '../types';

const EVT_STATE = 'ren://state-changed';
const EVT_TRANSCRIPT = 'ren://transcript';
const EVT_DOWNLOAD = 'ren://download-progress';
const EVT_ERROR = 'ren://error';
const EVT_WAVEFORM = 'ren://waveform';
const EVT_TOOL_EXECUTING = 'ren://tool-executing';
const EVT_TOOL_RESULT = 'ren://tool-result';
const EVT_OPEN_SETTINGS = 'ren://open-settings';
const EVT_CLIPBOARD_ARMED = 'ren://clipboard-armed';

export const useRenEvents = () => {
  // Stable action references — Zustand guarantees these are referentially stable,
  // so selecting them individually avoids re-subscribing on state updates.
  const setState = useRenStore((s) => s.setState);
  const setError = useRenStore((s) => s.setError);
  const setTranscript = useRenStore((s) => s.setTranscript);
  const setDownloadProgress = useRenStore((s) => s.setDownloadProgress);
  const setWaveform = useRenStore((s) => s.setWaveform);
  const setToolActivity = useRenStore((s) => s.setToolActivity);
  const setSettingsOpen = useRenStore((s) => s.setSettingsOpen);
  const setClipboardPreview = useRenStore((s) => s.setClipboardPreview);

  const transcriptTimer = useRef<number | null>(null);
  const toolCardTimer = useRef<number | null>(null);

  useEffect(() => {
    const unlisten: Array<() => void> = [];
    let cancelled = false;

    const clearTranscriptTimer = () => {
      if (transcriptTimer.current !== null) {
        window.clearTimeout(transcriptTimer.current);
        transcriptTimer.current = null;
      }
    };

    const clearToolCardTimer = () => {
      if (toolCardTimer.current !== null) {
        window.clearTimeout(toolCardTimer.current);
        toolCardTimer.current = null;
      }
    };

    const setup = async () => {
      const [
        stateOff,
        transcriptOff,
        downloadOff,
        errorOff,
        waveOff,
        toolExecOff,
        toolResOff,
        settingsOff,
        clipboardOff,
      ] = await Promise.all([
        listen<StateChangedPayload>(EVT_STATE, (e) => setState(e.payload.state)),

        listen<TranscriptPayload>(EVT_TRANSCRIPT, (e) => {
          if (!e.payload.is_final) return;
          setTranscript(e.payload.text);
          clearTranscriptTimer();
          transcriptTimer.current = window.setTimeout(() => {
            setTranscript(null);
            transcriptTimer.current = null;
          }, TRANSCRIPT_VISIBLE_MS);
        }),

        listen<DownloadProgressPayload>(EVT_DOWNLOAD, (e) => {
          const p = e.payload;
          setDownloadProgress({
            step: p.step,
            downloadedBytes: p.downloaded_bytes,
            totalBytes: p.total_bytes,
            speedBps: p.speed_bps,
          });
        }),

        listen<ErrorPayload>(EVT_ERROR, (e) =>
          setError({
            code: e.payload.code,
            message: e.payload.message,
            timestamp: Date.now(),
          })
        ),

        listen<WaveformPayload>(EVT_WAVEFORM, (e) => setWaveform(e.payload.amplitudes)),

        listen<ToolExecutingPayload>(EVT_TOOL_EXECUTING, (e) => {
          clearToolCardTimer();
          setToolActivity({
            tool: e.payload.tool,
            status: 'running',
            message: e.payload.description,
            startedAt: Date.now(),
          });
        }),

        listen<ToolResultPayload>(EVT_TOOL_RESULT, (e) => {
          setToolActivity({
            tool: e.payload.tool,
            status: e.payload.success ? 'success' : 'failure',
            message: e.payload.summary,
            startedAt: Date.now(),
          });
          clearToolCardTimer();
          toolCardTimer.current = window.setTimeout(() => {
            setToolActivity(null);
            toolCardTimer.current = null;
          }, TOOL_CARD_VISIBLE_MS);
        }),

        listen(EVT_OPEN_SETTINGS, () => setSettingsOpen(true)),

        listen<{ preview: string | null }>(EVT_CLIPBOARD_ARMED, (e) => {
          setClipboardPreview(e.payload.preview ?? null);
        }),
      ]);

      // If unmounted while awaiting, drop subscriptions immediately.
      if (cancelled) {
        stateOff();
        transcriptOff();
        downloadOff();
        errorOff();
        waveOff();
        toolExecOff();
        toolResOff();
        settingsOff();
        clipboardOff();
        return;
      }

      unlisten.push(
        stateOff,
        transcriptOff,
        downloadOff,
        errorOff,
        waveOff,
        toolExecOff,
        toolResOff,
        settingsOff,
        clipboardOff,
      );
    };

    setup();

    return () => {
      cancelled = true;
      clearTranscriptTimer();
      clearToolCardTimer();
      unlisten.forEach((off) => off());
    };
  }, [
    setState,
    setError,
    setTranscript,
    setDownloadProgress,
    setWaveform,
    setToolActivity,
    setSettingsOpen,
    setClipboardPreview,
  ]);
};
