/**
 * Orb
 * Core visual element. A fractal-noise distorted sphere renders the core;
 * modifier classes tune colour/glow per state. Speaking state draws bars
 * whose height is driven by backend RMS amplitudes.
 */

import { motion } from 'framer-motion';
import { useRenStore } from '../store';
import type { RenState } from '../types';
import { WAVEFORM_BAR_COUNT } from '../config/ui';
import styles from './Orb.module.css';

const PARTICLE_COUNT = 4;
const WAVEFORM_MIN_SCALE = 0.15;

const OrbCore = () => (
  <svg
    className={styles.core}
    viewBox="0 0 100 100"
    aria-hidden="true"
    preserveAspectRatio="xMidYMid meet"
  >
    <defs>
      <radialGradient id="ren-orb-fill" cx="50%" cy="50%" r="50%">
        <stop offset="0%" stopColor="rgba(180, 240, 255, 0.95)" />
        <stop offset="40%" stopColor="rgba(0, 200, 240, 0.65)" />
        <stop offset="75%" stopColor="rgba(0, 140, 200, 0.35)" />
        <stop offset="100%" stopColor="rgba(0, 80, 140, 0)" />
      </radialGradient>

      <radialGradient id="ren-orb-rim" cx="50%" cy="50%" r="50%">
        <stop offset="70%" stopColor="rgba(0, 240, 255, 0)" />
        <stop offset="92%" stopColor="rgba(0, 240, 255, 0.85)" />
        <stop offset="100%" stopColor="rgba(0, 240, 255, 0)" />
      </radialGradient>

      <filter
        id="ren-orb-distort"
        x="-60%"
        y="-60%"
        width="220%"
        height="220%"
        filterUnits="objectBoundingBox"
      >
        <feTurbulence
          type="fractalNoise"
          baseFrequency="0.038"
          numOctaves="3"
          seed="4"
          result="noise"
        />
        <feDisplacementMap
          in="SourceGraphic"
          in2="noise"
          scale="10"
          xChannelSelector="R"
          yChannelSelector="G"
        />
      </filter>

      <filter
        id="ren-orb-mesh"
        x="-10%"
        y="-10%"
        width="120%"
        height="120%"
        filterUnits="objectBoundingBox"
      >
        <feTurbulence
          type="fractalNoise"
          baseFrequency="0.9"
          numOctaves="2"
          seed="7"
          result="fine"
        />
        <feColorMatrix
          in="fine"
          type="matrix"
          values="0 0 0 0 0.55
                  0 0 0 0 0.95
                  0 0 0 0 1
                  0 0 0 1.2 -0.6"
          result="tinted"
        />
        <feComposite in="tinted" in2="SourceGraphic" operator="in" />
      </filter>
    </defs>

    <circle cx="50" cy="50" r="42" fill="url(#ren-orb-fill)" />
    <circle
      cx="50"
      cy="50"
      r="42"
      fill="url(#ren-orb-fill)"
      filter="url(#ren-orb-distort)"
      opacity="0.85"
    />
    <circle
      cx="50"
      cy="50"
      r="42"
      fill="url(#ren-orb-fill)"
      filter="url(#ren-orb-mesh)"
      opacity="0.55"
    />
    <circle
      cx="50"
      cy="50"
      r="44"
      fill="url(#ren-orb-rim)"
      filter="url(#ren-orb-distort)"
    />
  </svg>
);

const StateVisual = ({
  state,
  amplitudes,
}: {
  state: RenState;
  amplitudes: number[];
}) => {
  if (state === 'thinking') {
    return (
      <div className={styles.particles}>
        {Array.from({ length: PARTICLE_COUNT }, (_, i) => (
          <div key={i} className={styles.particle} />
        ))}
      </div>
    );
  }

  if (state === 'speaking') {
    return (
      <div className={styles.waveform}>
        {Array.from({ length: WAVEFORM_BAR_COUNT }, (_, i) => {
          const amp = amplitudes[i] ?? 0;
          const scale = Math.max(WAVEFORM_MIN_SCALE, amp);
          return (
            <div
              key={i}
              className={styles.waveBar}
              style={{ transform: `scaleY(${scale})` }}
            />
          );
        })}
      </div>
    );
  }

  return null;
};

export const Orb = () => {
  const currentState = useRenStore((s) => s.currentState);
  const waveformAmplitudes = useRenStore((s) => s.waveformAmplitudes);

  return (
    <div className={styles.container}>
      <motion.div
        data-tauri-drag-region
        className={`${styles.orb} ${styles[currentState]}`}
        initial={{ scale: 0, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        transition={{ duration: 0.5, ease: 'easeOut' }}
      >
        <OrbCore />
        <StateVisual state={currentState} amplitudes={waveformAmplitudes} />
      </motion.div>
    </div>
  );
};
