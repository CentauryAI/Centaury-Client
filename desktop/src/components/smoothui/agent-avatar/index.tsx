"use client";

import { cn } from "@/lib/utils";
import { useEffect, useRef } from "react";

export type AgentAvatarProps = Omit<
  React.CanvasHTMLAttributes<HTMLCanvasElement>,
  "children"
> & {
  /** String seed to generate a unique deterministic avatar pattern */
  seed: string;
  /** Diameter in pixels */
  size?: number;
  /** Enable pixel animation (respects prefers-reduced-motion) */
  animated?: boolean;
};

const GRID_SIZE = 6;

/** Pulse: each pixel oscillates lightness independently */
const PULSE_SPEED = 0.002;
const PULSE_AMPLITUDE = 8;

/** Breathe: global slow scale oscillation */
const BREATHE_SPEED = 0.001;
const BREATHE_AMPLITUDE = 4;

/** Wave: diagonal sweep across the grid */
const WAVE_SPEED = 0.0015;
const WAVE_AMPLITUDE = 6;
const WAVE_LENGTH = 3;

/** Sparkle: random bright flashes */
const SPARKLE_SPEED = 0.004;
const SPARKLE_THRESHOLD = 0.92;
const SPARKLE_BOOST = 8;

/** Scale pulse: whole avatar breathes in size */
const SCALE_PULSE_SPEED = 0.0008;
const SCALE_PULSE_AMOUNT = 0.03;

const GLOW_RADIUS_RATIO = 0.2;

// Slate/brand ramp — the only allowed colors per RULES.md §3.
// No hue variation: all slots come from these hex values.
const SLATE_RAMP = [
  "#2c2e3a",
  "#41465e",
  "#595e7a",
  "#6c7296",
  "#777fa8",
  "#959fbf",
  "#abb6ce",
  "#c9d1e0",
  "#dfe6f5",
] as const;

/** Simple deterministic hash from a string */
const hashSeed = (str: string): number => {
  let hash = 0;
  for (const char of str) {
    hash = ((hash << 5) - hash + char.charCodeAt(0)) | 0;
  }
  return Math.abs(hash);
};

/** Seeded PRNG (mulberry32) */
const createRng = (seed: number) => {
  let state = seed;
  return () => {
    state = (state + 0x6d_2b_79_f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
};

/** Parse a hex color string to [r, g, b] numbers 0-255 */
function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ];
}

/** Interpolate between two [r,g,b] colors by t in [0,1] */
function lerpRgb(
  a: [number, number, number],
  b: [number, number, number],
  t: number
): [number, number, number] {
  return [
    Math.round(a[0] + (b[0] - a[0]) * t),
    Math.round(a[1] + (b[1] - a[1]) * t),
    Math.round(a[2] + (b[2] - a[2]) * t),
  ];
}

type SlateColor = {
  baseIdx: number;   // index into SLATE_RAMP
  phase: number;
  brightness: number;
  sparklePhase: number;
};

type Cell = SlateColor;

/** Derive a 3-step palette within the slate ramp — anchored to seed */
const generatePalette = (hash: number): [number, number, number] => {
  const rng = createRng(hash);
  // Three anchor indices spread across the ramp, seed-shifted
  const base = Math.floor(rng() * (SLATE_RAMP.length - 4));
  return [base, base + 2, base + 4] as [number, number, number];
};

/** Build a grid with per-cell metadata */
const generateGrid = (hash: number): Cell[][] => {
  const rng = createRng(hash + 1);
  const grid: Cell[][] = [];

  for (let y = 0; y < GRID_SIZE; y++) {
    grid[y] = [];
    for (let x = 0; x < GRID_SIZE; x++) {
      grid[y][x] = {
        baseIdx: Math.floor(rng() * 3), // picks one of the 3 palette slots
        phase: rng() * Math.PI * 2,
        brightness: 0.6 + rng() * 0.4,
        sparklePhase: rng() * Math.PI * 2,
      };
    }
  }

  return grid;
};

const AgentAvatar = ({
  seed,
  size = 64,
  animated = true,
  className,
  ...props
}: AgentAvatarProps) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rafRef = useRef<number>(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }

    const ctx = canvas.getContext("2d");
    if (!ctx) {
      return;
    }

    const dpr = window.devicePixelRatio || 1;
    canvas.width = size * dpr;
    canvas.height = size * dpr;
    ctx.scale(dpr, dpr);

    const hash = hashSeed(seed);
    const paletteIdxs = generatePalette(hash);
    const grid = generateGrid(hash);
    const cellSize = size / GRID_SIZE;
    const half = size / 2;

    // The 3 ramp colors this seed uses
    const paletteColors = paletteIdxs.map((i) => hexToRgb(SLATE_RAMP[i])) as [
      [number, number, number],
      [number, number, number],
      [number, number, number],
    ];

    const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    let shouldAnimate = animated && !motionQuery.matches;

    const draw = (time: number) => {
      ctx.clearRect(0, 0, size, size);

      // Scale pulse — whole avatar breathes
      const scale = shouldAnimate
        ? 1 + Math.sin(time * SCALE_PULSE_SPEED) * SCALE_PULSE_AMOUNT
        : 1;

      ctx.save();
      ctx.translate(half, half);
      ctx.scale(scale, scale);
      ctx.translate(-half, -half);

      // Clip to circle
      ctx.beginPath();
      ctx.arc(half, half, half, 0, Math.PI * 2);
      ctx.clip();

      // Dark background from the ramp
      const [br, bg, bb] = hexToRgb(SLATE_RAMP[1]);
      ctx.fillStyle = `rgb(${br},${bg},${bb})`;
      ctx.fillRect(0, 0, size, size);

      // Global breathe offset for blending
      const breatheOffset = shouldAnimate
        ? Math.sin(time * BREATHE_SPEED) * BREATHE_AMPLITUDE
        : 0;

      // Draw pixel grid
      for (let y = 0; y < GRID_SIZE; y++) {
        for (let x = 0; x < GRID_SIZE; x++) {
          const cell = grid[y][x];
          const baseRgb = paletteColors[cell.baseIdx];

          // Per-pixel pulse — slightly blends toward a lighter ramp step
          const pulse = shouldAnimate
            ? Math.sin(time * PULSE_SPEED + cell.phase) * PULSE_AMPLITUDE
            : 0;

          // Diagonal wave sweep
          const waveDist = (x + y) / WAVE_LENGTH;
          const wave = shouldAnimate
            ? Math.sin(time * WAVE_SPEED + waveDist) * WAVE_AMPLITUDE
            : 0;

          // Sparkle — occasional lighter flash
          const sparkleVal = shouldAnimate
            ? Math.sin(time * SPARKLE_SPEED + cell.sparklePhase)
            : 0;
          const sparkle =
            sparkleVal > SPARKLE_THRESHOLD
              ? ((sparkleVal - SPARKLE_THRESHOLD) / (1 - SPARKLE_THRESHOLD)) *
                SPARKLE_BOOST
              : 0;

          // Lightness-blend: combine offsets into a 0-1 lerp toward a lighter step
          const rawT = ((pulse + breatheOffset + wave + sparkle) / 30 + 0.5) * cell.brightness;
          const t = Math.max(0, Math.min(1, rawT));

          // Blend between base color and one step lighter (capped at ramp end)
          const nextIdx = Math.min(
            paletteIdxs[cell.baseIdx] + 2,
            SLATE_RAMP.length - 1
          );
          const nextRgb = hexToRgb(SLATE_RAMP[nextIdx]);
          const [r, g, b] = lerpRgb(baseRgb, nextRgb, t);

          ctx.shadowColor = `rgb(${r},${g},${b})`;
          ctx.shadowBlur = cellSize * 0.35;

          ctx.fillStyle = `rgb(${r},${g},${b})`;
          ctx.fillRect(x * cellSize, y * cellSize, cellSize, cellSize);
        }
      }

      // Reset shadow before restore
      ctx.shadowBlur = 0;
      ctx.restore();

      // Outer ring from the lightest palette color
      const [gr, gg, gb] = paletteColors[2];
      ctx.save();
      ctx.globalCompositeOperation = "screen";
      ctx.shadowColor = `rgba(${gr},${gg},${gb},0.4)`;
      ctx.shadowBlur = size * GLOW_RADIUS_RATIO;
      ctx.beginPath();
      ctx.arc(half, half, half - 1, 0, Math.PI * 2);
      ctx.strokeStyle = `rgba(${gr},${gg},${gb},0.12)`;
      ctx.lineWidth = 2;
      ctx.stroke();
      ctx.restore();

      if (shouldAnimate) {
        rafRef.current = requestAnimationFrame(draw);
      }
    };

    const handleMotionChange = () => {
      cancelAnimationFrame(rafRef.current);
      shouldAnimate = animated && !motionQuery.matches;
      if (shouldAnimate) {
        rafRef.current = requestAnimationFrame(draw);
      } else {
        draw(0);
      }
    };

    motionQuery.addEventListener("change", handleMotionChange);

    if (shouldAnimate) {
      rafRef.current = requestAnimationFrame(draw);
    } else {
      draw(0);
    }

    return () => {
      cancelAnimationFrame(rafRef.current);
      motionQuery.removeEventListener("change", handleMotionChange);
    };
  }, [seed, size, animated]);

  return (
    <canvas
      aria-label={`Avatar for ${seed}`}
      className={cn("rounded-full", className)}
      ref={canvasRef}
      role="img"
      style={{ width: size, height: size }}
      {...props}
    />
  );
};

export default AgentAvatar;
