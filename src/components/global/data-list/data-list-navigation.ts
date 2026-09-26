/**
 * SOURCE OF TRUTH KEYWORDS: DataList keyboard navigation, nextActiveIndex, list keys, ArrowDown ArrowUp Home End PageDown PageUp, clamp index
 * WHAT:  `nextActiveIndex(key, current, count, pageSize)`: the row a navigation key moves the active row to, or
 *        null when the key is not a navigation key (or the list is empty). `clampIndex` keeps an index in range.
 * WHY:   Pure, so the key table is tested without a DOM or a virtualizer, and DataList only wires it to focus and
 *        scrolling. Movement stops at the ends (no wrap-around): at 100k rows a wrap from the top to the bottom is a
 *        disorienting jump, and Home/End already reach the ends in one key. Page keys move by the rows that fit in
 *        the viewport, at least one.
 * WHERE: DataList.tsx (onKeyDown); data-list-navigation.test.ts.
 */

/** The keys DataList navigates with. */
export const NAVIGATION_KEYS = ["ArrowDown", "ArrowUp", "Home", "End", "PageDown", "PageUp"] as const;

export type NavigationKey = (typeof NAVIGATION_KEYS)[number];

export function isNavigationKey(key: string): key is NavigationKey {
  const keys: readonly string[] = NAVIGATION_KEYS;
  return keys.includes(key);
}

/** `index` limited to the rows that exist (0 when there are none). */
export function clampIndex(index: number, count: number): number {
  return Math.max(0, Math.min(index, count - 1));
}

export function nextActiveIndex(key: string, current: number, count: number, pageSize: number): number | null {
  if (count === 0 || !isNavigationKey(key)) {
    return null;
  }
  const page = Math.max(1, Math.floor(pageSize));
  const moves: Readonly<Record<NavigationKey, number>> = {
    ArrowDown: current + 1,
    ArrowUp: current - 1,
    Home: 0,
    End: count - 1,
    PageDown: current + page,
    PageUp: current - page,
  };
  return clampIndex(moves[key], count);
}
