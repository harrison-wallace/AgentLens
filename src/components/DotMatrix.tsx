import type { CSSProperties } from "react";
import "./dotmatrix.css";

/** Default cycle length at speed 1. Concrete ms — never a CSS calc product. */
const BASE_DURATION_MS = 1_400;

/**
 * Ring index for a 5×5 cell (row-major). 0 = centre, 1 = inner, 2 = outer.
 * Ripple delays by ring so the pulse expands from the middle.
 */
const RING: readonly number[] = (() => {
  const out: number[] = [];
  for (let i = 0; i < 25; i += 1) {
    const r = Math.floor(i / 5);
    const c = i % 5;
    const dist = Math.max(Math.abs(r - 2), Math.abs(c - 2));
    out.push(dist);
  }
  return out;
})();

/**
 * Outer-ring walk order, clockwise from top-left. Snake lights one cell at
 * a time along this path; non-ring cells get no animation delay role.
 */
const OUTER_RING: readonly number[] = [0, 1, 2, 3, 4, 9, 14, 19, 24, 23, 22, 21, 20, 15, 10, 5];

const OUTER_INDEX: ReadonlyMap<number, number> = new Map(OUTER_RING.map((cell, i) => [cell, i]));

export type DotMatrixVariant = "ripple" | "snake";

export interface DotMatrixProps {
  variant: DotMatrixVariant;
  /**
   * CSS colour reference applied as `color` so dots use `currentColor`.
   * Pass a token such as `var(--color-agent-working)` — never a hex literal.
   */
  color: string;
  /** False = static rest frame (the visual vocabulary for "stopped"). */
  animated: boolean;
  /** Multiplier on the base cycle; higher = faster. Default 1. */
  speed?: number;
  /** Soft glow via drop-shadow in currentColor. */
  bloom?: boolean;
  /** Outer box edge in CSS px. Default 16. */
  size?: number;
  /** Each dot's diameter in CSS px. Default 2. */
  dotSize?: number;
  ariaLabel: string;
}

/**
 * 5×5 activity indicator. Colour = state, variant = which agent, speed/bloom
 * = intensity. Opacity-only animation; no per-frame React work.
 *
 * Written here rather than vendored: the public MIT repo cannot ship an
 * unlicensed dependency, and WebKitGTK needs concrete ms animation timings.
 */
export default function DotMatrix({
  variant,
  color,
  animated,
  speed = 1,
  bloom = false,
  size = 16,
  dotSize = 2,
  ariaLabel,
}: DotMatrixProps) {
  // Clamp so a zero or negative never yields 0ms and freezes WebKit.
  const safeSpeed = Number.isFinite(speed) && speed > 0 ? speed : 1;
  const durationMs = Math.max(200, Math.round(BASE_DURATION_MS / safeSpeed));
  const ringCount = 3;
  const outerLen = OUTER_RING.length;

  const className = [
    "dot-matrix",
    `dot-matrix--${variant}`,
    animated ? "dot-matrix--animated" : "",
    bloom ? "dot-matrix--bloom" : "",
  ]
    .filter(Boolean)
    .join(" ");

  const gap = Math.max(0, (size - 5 * dotSize) / 4);

  return (
    <span
      className={className}
      role="img"
      aria-label={ariaLabel}
      style={{
        color,
        width: size,
        height: size,
        gap,
      }}
    >
      {Array.from({ length: 25 }, (_, i) => {
        const style: CSSProperties = {
          width: dotSize,
          height: dotSize,
        };

        if (animated) {
          // Concrete ms strings only — WebKit freezes on var() arithmetic in
          // animation-duration / animation-delay.
          style.animationDuration = `${durationMs}ms`;
          if (variant === "ripple") {
            const ring = RING[i] ?? 0;
            const delay = Math.round((ring / ringCount) * durationMs);
            style.animationDelay = `${delay}ms`;
          } else {
            const pos = OUTER_INDEX.get(i);
            if (pos === undefined) {
              // Inner cells stay at rest opacity; no named animation peak.
              style.animationName = "none";
              style.opacity = 0.12;
            } else {
              const delay = Math.round((pos / outerLen) * durationMs);
              style.animationDelay = `-${delay}ms`;
            }
          }
        }

        return <span key={i} className="dot-matrix__dot" style={style} />;
      })}
    </span>
  );
}
