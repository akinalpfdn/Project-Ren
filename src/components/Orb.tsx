/**
 * Orb
 * Core visual element. Now using a Three.js WebGL canvas for a
 * dynamic, reactive 3D wireframe mesh driven by states and audio levels.
 */

import { motion } from 'framer-motion';
import { useEffect, useRef } from 'react';
import * as THREE from 'three';
import { useRenStore } from '../store';
import type { RenState } from '../types';
import { WAVEFORM_BAR_COUNT } from '../config/ui';
import styles from './Orb.module.css';

const PARTICLE_COUNT = 4;
const WAVEFORM_MIN_SCALE = 0.15;

const vertexShader = `
uniform float uTime;
uniform float uSpeed;
uniform float uNoiseDensity;
uniform float uNoiseStrength;

varying vec3 vPosition;

// Simplex 3D Noise loosely based on classic implementations
vec4 permute(vec4 x){return mod(((x*34.0)+1.0)*x, 289.0);}
vec4 taylorInvSqrt(vec4 r){return 1.79284291400159 - 0.85373472095314 * r;}
float snoise(vec3 v){ 
  const vec2  C = vec2(1.0/6.0, 1.0/3.0);
  const vec4  D = vec4(0.0, 0.5, 1.0, 2.0);
  vec3 i  = floor(v + dot(v, C.yyy));
  vec3 x0 = v - i + dot(i, C.xxx);
  vec3 g = step(x0.yzx, x0.xyz);
  vec3 l = 1.0 - g;
  vec3 i1 = min( g.xyz, l.zxy );
  vec3 i2 = max( g.xyz, l.zxy );
  vec3 x1 = x0 - i1 + 1.0 * C.xxx;
  vec3 x2 = x0 - i2 + 2.0 * C.xxx;
  vec3 x3 = x0 - 1.0 + 3.0 * C.xxx;
  i = mod(i, 289.0 ); 
  vec4 p = permute( permute( permute( 
             i.z + vec4(0.0, i1.z, i2.z, 1.0 ))
           + i.y + vec4(0.0, i1.y, i2.y, 1.0 )) 
           + i.x + vec4(0.0, i1.x, i2.x, 1.0 ));
  float n_ = 1.0/7.0; 
  vec3  ns = n_ * D.wyz - D.xzx;
  vec4 j = p - 49.0 * floor(p * ns.z *ns.z); 
  vec4 x_ = floor(j * ns.z);
  vec4 y_ = floor(j - 7.0 * x_ ); 
  vec4 x = x_ *ns.x + ns.yyyy;
  vec4 y = y_ *ns.x + ns.yyyy;
  vec4 h = 1.0 - abs(x) - abs(y);
  vec4 b0 = vec4( x.xy, y.xy );
  vec4 b1 = vec4( x.zw, y.zw );
  vec4 s0 = floor(b0)*2.0 + 1.0;
  vec4 s1 = floor(b1)*2.0 + 1.0;
  vec4 sh = -step(h, vec4(0.0));
  vec4 a0 = b0.xzyw + s0.xzyw*sh.xxyy ;
  vec4 a1 = b1.xzyw + s1.xzyw*sh.zzww ;
  vec3 p0 = vec3(a0.xy,h.x);
  vec3 p1 = vec3(a0.zw,h.y);
  vec3 p2 = vec3(a1.xy,h.z);
  vec3 p3 = vec3(a1.zw,h.w);
  vec4 norm = taylorInvSqrt(vec4(dot(p0,p0), dot(p1,p1), dot(p2, p2), dot(p3,p3)));
  p0 *= norm.x;
  p1 *= norm.y;
  p2 *= norm.z;
  p3 *= norm.w;
  vec4 m = max(0.6 - vec4(dot(x0,x0), dot(x1,x1), dot(x2,x2), dot(x3,x3)), 0.0);
  m = m * m;
  return 42.0 * dot( m*m, vec4( dot(p0,x0), dot(p1,x1), 
                                dot(p2,x2), dot(p3,x3) ) );
}

void main() {
  float noise = snoise(position * uNoiseDensity + uTime * uSpeed);
  vec3 newPosition = position + normal * noise * uNoiseStrength;
  vPosition = newPosition;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(newPosition, 1.0);
}
`;

const fragmentShader = `
uniform vec3 uColorTop;
uniform vec3 uColorBottom;
varying vec3 vPosition;

void main() {
  float yMapping = (vPosition.y + 1.0) / 2.0; 
  yMapping = clamp(yMapping, 0.0, 1.0);
  vec3 color = mix(uColorBottom, uColorTop, yMapping);
  gl_FragColor = vec4(color, 1.0);
}
`;

const OrbCore = ({ state, amplitudes }: { state: RenState; amplitudes: number[] }) => {
  const mountRef = useRef<HTMLDivElement>(null);
  const materialRef = useRef<THREE.ShaderMaterial>(null);
  const targetsRef = useRef({ strength: 0.2, speed: 0.5 });

  useEffect(() => {
    if (!mountRef.current) return;

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(75, 1, 0.1, 100);
    camera.position.z = 3;

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(window.devicePixelRatio);

    // Explicitly force the canvas to fill the parent container natively
    renderer.domElement.style.width = '100%';
    renderer.domElement.style.height = '100%';
    renderer.domElement.style.display = 'block';
    renderer.domElement.style.position = 'absolute';
    renderer.domElement.style.top = '0';
    renderer.domElement.style.left = '0';
    renderer.domElement.style.pointerEvents = 'none';

    mountRef.current.appendChild(renderer.domElement);

    const geometry = new THREE.IcosahedronGeometry(1.1, 9);

    const material = new THREE.ShaderMaterial({
      vertexShader,
      fragmentShader,
      wireframe: true,
      uniforms: {
        uTime: { value: 0 },
        uSpeed: { value: 0.5 },
        uNoiseDensity: { value: 1.5 },
        uNoiseStrength: { value: 0.2 },
        uColorTop: { value: new THREE.Color("#00bbbb") }, // Darker Cyan
        uColorBottom: { value: new THREE.Color("#00bbbb") } // Darker Cyan
      }
    });
    materialRef.current = material;

    const sphere = new THREE.Mesh(geometry, material);
    scene.add(sphere);

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        // Read actual element size to prevent fractional scaling bugs
        const width = entry.target.clientWidth;
        const height = entry.target.clientHeight;
        if (width === 0 || height === 0) continue;

        // Pass false to prevent Three.js from setting inline style width/height in px
        renderer.setSize(width, height, false);
        camera.aspect = width / height;
        camera.updateProjectionMatrix();
      }
    });
    observer.observe(mountRef.current);

    const clock = new THREE.Clock();
    let animationFrameId: number;

    const animate = () => {
      animationFrameId = requestAnimationFrame(animate);

      if (materialRef.current) {
        const m = materialRef.current;
        m.uniforms.uSpeed.value += (targetsRef.current.speed - m.uniforms.uSpeed.value) * 0.1;
        m.uniforms.uNoiseStrength.value += (targetsRef.current.strength - m.uniforms.uNoiseStrength.value) * 0.1;
        // uTime is accumulated over frame times scaled by smoothed speed
        m.uniforms.uTime.value += clock.getDelta() * m.uniforms.uSpeed.value;
      }

      sphere.rotation.y += 0.002;
      sphere.rotation.x += 0.001;

      renderer.render(scene, camera);
    };

    // First frame initialization
    clock.start();
    animate();

    return () => {
      cancelAnimationFrame(animationFrameId);
      observer.disconnect();
      if (mountRef.current) {
        mountRef.current.removeChild(renderer.domElement);
      }
      geometry.dispose();
      material.dispose();
      renderer.dispose();
    };
  }, []);

  // Update target parameters based on state & audio amplitudes
  useEffect(() => {
    let targetSpeed = 0.5;
    let targetStrength = 0.2;

    switch (state) {
      case 'idle':
      case 'sleeping':
        targetSpeed = 0.2;
        targetStrength = 0.05;
        break;
      case 'listening':
        targetSpeed = 1.2;
        targetStrength = 0.3;
        break;
      case 'thinking':
        targetSpeed = 2.5;
        targetStrength = 0.15;
        break;
      case 'speaking': {
        const maxAmp = amplitudes.length > 0 ? Math.max(...amplitudes) : 0;
        targetSpeed = 1.0 + maxAmp * 3.5;
        targetStrength = 0.15 + maxAmp * 0.6;
        break;
      }
      case 'initializing':
      case 'waking':
        targetSpeed = 0.8;
        targetStrength = 0.15;
        break;
      case 'error':
        targetSpeed = 3.0;
        targetStrength = -0.1; // weird distortion for error
        break;
    }

    targetsRef.current = { speed: targetSpeed, strength: targetStrength };
  }, [state, amplitudes]);

  return <div ref={mountRef} className={styles.core} aria-hidden="true" />;
};

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

  // Removing standard waveform as voice now actively distorts 3D mesh
  // If the user still wants the waveform, they can uncomment this:
  /*
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
  */

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
        <OrbCore state={currentState} amplitudes={waveformAmplitudes} />
        <StateVisual state={currentState} amplitudes={waveformAmplitudes} />
      </motion.div>
    </div>
  );
};
