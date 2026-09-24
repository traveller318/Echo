/**
 * SOURCE OF TRUTH KEYWORDS: progressFraction, progress clamp, value over max, download fraction
 * WHAT:  `progressFraction(value, max)`: `value / max` clamped to 0…1; 0 for a non-positive max or a non-finite value.
 * WHY:   The bar is a CSS scale, so a stray value (bytes past the total while a hash runs) must never overdraw
 *        or produce NaN; kept apart from ProgressBar.tsx because a component file may export components alone.
 * WHERE: ProgressBar.tsx; callers that show a percentage next to the bar.
 */
/** `value` as a 0…1 fraction of `max`, clamped. */
export function progressFraction(value: number, max: number): number {
  if (!(max > 0) || !Number.isFinite(value)) {
    return 0;
  }
  return Math.min(Math.max(value / max, 0), 1);
}

