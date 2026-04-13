/**
 * Human-readable number formatters.
 */

const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const;
const BYTE_STEP = 1024;

/** Formats a byte count into the largest non-fractional unit (1 decimal place). */
export const formatBytes = (bytes: number): string => {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const exponent = Math.min(
    BYTE_UNITS.length - 1,
    Math.floor(Math.log(bytes) / Math.log(BYTE_STEP))
  );
  const value = bytes / Math.pow(BYTE_STEP, exponent);
  return `${value.toFixed(1)} ${BYTE_UNITS[exponent]}`;
};

/** Formats a transfer rate in bytes-per-second. Returns an empty string for zero. */
export const formatSpeed = (bytesPerSecond: number): string => {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return '';
  return `${formatBytes(bytesPerSecond)}/s`;
};
