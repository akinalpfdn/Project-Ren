/**
 * UI Constants
 * Single source of truth for values shared between TypeScript and the
 * backend contract. CSS-only concerns live in `styles/theme.css`.
 */

/** Number of bars in the speaking waveform. Must match backend RMS segment count. */
export const WAVEFORM_BAR_COUNT = 8;

/** How long a finalized transcript stays on screen before auto-clearing. */
export const TRANSCRIPT_VISIBLE_MS = 8_000;
