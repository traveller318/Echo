/**
 * SOURCE OF TRUTH KEYWORDS: CountdownRing, cancel countdown ring, draining ring, Esc countdown, pathLength
 * WHAT:  A `--ring-size` ring with a `--ring-stroke` stroke that drains linearly from the fraction left to empty over
 *        `remainingMs`.
 * WHY:   04 §4: while a cancel is pending the pill shows how long the user has to press Esc again. Rust sends the time
 *        left once, when the countdown starts, so the ring drains on its own clock; the full length is the time left
 *        when the ring first appeared (the countdown setting), so a pill that mounts late still drains from the
 *        right point. The drain is information, not decoration, so it runs under reduced motion too (linear, no
 *        spring). The circle geometry is in the SVG's own 0–20 units; the rendered size and stroke are tokens.
 * WHERE: src/pill/Pill.tsx (cancel layout).
 */
import { motion } from "motion/react";
import { useState } from "react";

export interface CountdownRingProps {
  /** Time left before the take is discarded, in ms. */
  readonly remainingMs: number;
}

const MS_PER_SECOND = 1000;

export function CountdownRing({ remainingMs }: CountdownRingProps) {
  const [totalMs] = useState(() => Math.max(remainingMs, 1));
  const left = Math.min(1, Math.max(0, remainingMs / totalMs));
  return (
    <svg
      data-slot="countdown-ring"
      viewBox="0 0 20 20"
      aria-hidden
      className="size-(--ring-size) shrink-0 stroke-(length:--ring-stroke)"
    >
      <circle cx="10" cy="10" r="9" fill="none" className="stroke-fill-pressed" />
      {/* Turned a quarter back in the SVG's own geometry, so the drain starts at twelve o'clock. */}
      <g transform="rotate(-90 10 10)">
        <motion.circle
          cx="10"
          cy="10"
          r="9"
          fill="none"
          strokeLinecap="round"
          className="stroke-fg"
          initial={{ pathLength: left }}
          animate={{ pathLength: 0 }}
          transition={{ duration: remainingMs / MS_PER_SECOND, ease: "linear" }}
        />
      </g>
    </svg>
  );
}
