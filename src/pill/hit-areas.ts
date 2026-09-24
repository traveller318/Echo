/**
 * SOURCE OF TRUTH KEYWORDS: PillHitAreaRegistry, PillHitAreasContext, usePillHitArea, pill click-through, button rects, pill_set_hit_areas
 * WHAT:  PillHitAreaRegistry: tracks the pill's clickable elements and reports their rectangles (CSS pixels of the
 *        page) whenever they change; PillHitAreasContext hands it to the tree; `usePillHitArea()` is the ref callback a
 *        button uses to be clickable.
 * WHY:   The pill window lets clicks through (04 §4) and Rust switches that off only over the rectangles the page
 *        reports (`pill_set_hit_areas`), so every button must report itself, after every morph and resize, and
 *        disappear from the list when it unmounts. Reports are deduplicated so a re-render sends nothing. The
 *        registry is plain TS (no React state), so measuring never re-renders the pill; ResizeObserver catches size
 *        changes and `measure()` is called when an animation settles (transforms move a button without resizing it).
 * WHERE: Created by src/pill/Pill.tsx; `usePillHitArea` is used by pill/_components/PillAction.tsx.
 */
import { createContext, useCallback, useContext } from "react";
import type { OverlayRect } from "@/bindings";

export type ReportHitAreas = (areas: OverlayRect[]) => void;

export class PillHitAreaRegistry {
  private readonly elements = new Set<Element>();
  private readonly observer: ResizeObserver | null;
  private last = "";

  constructor(private readonly report: ReportHitAreas) {
    this.observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(() => {
            this.measure();
          });
  }

  /** Tracks `element` until the returned function is called. */
  register(element: Element): () => void {
    this.elements.add(element);
    this.observer?.observe(element);
    this.measure();
    return () => {
      this.elements.delete(element);
      this.observer?.unobserve(element);
      this.measure();
    };
  }

  /** Reports the current rectangles if they changed since the last report. */
  measure(): void {
    const areas = [...this.elements].map((element) => {
      // Whole pixels, grown outward, so the reported area always covers the whole button.
      const rect = element.getBoundingClientRect();
      const left = Math.max(0, Math.floor(rect.left));
      const top = Math.max(0, Math.floor(rect.top));
      return {
        x: left,
        y: top,
        width: Math.max(0, Math.ceil(rect.right) - left),
        height: Math.max(0, Math.ceil(rect.bottom) - top),
      };
    });
    const key = JSON.stringify(areas);
    if (key !== this.last) {
      this.last = key;
      this.report(areas);
    }
  }

  /** Stops observing; the registry reports nothing afterwards. */
  dispose(): void {
    this.observer?.disconnect();
    this.elements.clear();
  }
}

export const PillHitAreasContext = createContext<PillHitAreaRegistry | null>(null);

/** A ref callback that makes its element one of the pill's clickable areas while it is mounted. */
export function usePillHitArea(): (element: Element | null) => (() => void) | undefined {
  const registry = useContext(PillHitAreasContext);
  return useCallback(
    (element: Element | null) => {
      if (element === null || registry === null) {
        return undefined;
      }
      return registry.register(element);
    },
    [registry],
  );
}
