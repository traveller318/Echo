/**
 * SOURCE OF TRUTH KEYWORDS: useDelayedFlag, delay loading, show after delay, --delay-loading, no flicker, keyed delay
 * WHAT:  `useDelayedFlag(key, delayMs)` is true once `key` has been non-null and unchanged for `delayMs`; false while
 *        `key` is null and again right after it changes.
 * WHY:   04 §1 "Nothing waits": a loading or processing state appears only after --delay-loading, so a fast path never
 *        flickers. Keying the wait (e.g. by take id) restarts it for every new wait and never carries a finished one
 *        over, without resetting state inside an effect.
 * WHERE: The pill (processing shimmer after --delay-loading); any view that delays a loading indicator.
 */
import { useEffect, useState } from "react";

export function useDelayedFlag(key: string | null, delayMs: number): boolean {
  const [reached, setReached] = useState<string | null>(null);
  useEffect(() => {
    if (key === null) {
      return undefined;
    }
    const timer = setTimeout(() => {
      setReached(key);
    }, delayMs);
    return () => {
      clearTimeout(timer);
    };
  }, [key, delayMs]);
  return key !== null && reached === key;
}
