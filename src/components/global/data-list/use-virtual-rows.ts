/**
 * SOURCE OF TRUTH KEYWORDS: useVirtualRows, TanStack Virtual core, Virtualizer binding, list virtualization, measure rows, flushSync on scroll
 * WHAT:  `useVirtualRows(options)`: a TanStack Virtual `Virtualizer` for a scrolling element, bound to React: the
 *        component re-renders whenever the visible range, a measured size or the scroll changes, and the returned
 *        instance answers `getVirtualItems()`, `getTotalSize()`, `scrollToIndex()` and `measureElement`.
 * WHY:   The same binding `@tanstack/react-virtual`'s useVirtualizer performs (options every render, mount and
 *        update effects, a synchronous re-render on scroll so rows never lag the scrollbar, none while measuring
 *        inside a commit), minus its direct-DOM mode Echo does not use. It exists because useVirtualizer is on
 *        React Compiler's incompatible-library list, which `react-hooks/incompatible-library` reports and the gate
 *        does not allow (`--max-warnings 0`, no inline disables). The instance's answers change while its identity
 *        does not, so the hook and its callers opt out of compiler memoization ("use no memo"); the build does not
 *        run the compiler today, so this only keeps a future switch safe. The core comes from the package's own
 *        re-export, so there is still one TanStack Virtual dependency (02 §2.4).
 * WHERE: DataList.tsx.
 */
import {
  elementScroll,
  observeElementOffset,
  observeElementRect,
  Virtualizer,
  type VirtualizerOptions,
} from "@tanstack/react-virtual";
import { useLayoutEffect, useReducer, useState } from "react";
import { flushSync } from "react-dom";

export type VirtualRowsOptions = Pick<
  VirtualizerOptions<HTMLDivElement, HTMLDivElement>,
  "count" | "getScrollElement" | "estimateSize" | "overscan" | "getItemKey"
>;

export type VirtualRows = Virtualizer<HTMLDivElement, HTMLDivElement>;

export function useVirtualRows(options: VirtualRowsOptions): VirtualRows {
  "use no memo";
  const [, rerender] = useReducer((version: number) => version + 1, 0);
  const [binding] = useState(() => {
    // A size measured inside a commit must not flush another render synchronously.
    let measuring = false;
    const onChange = (_instance: VirtualRows, sync: boolean) => {
      if (sync && !measuring) {
        flushSync(rerender);
      } else {
        rerender();
      }
    };
    const virtualizer = new Virtualizer<HTMLDivElement, HTMLDivElement>({
      observeElementRect,
      observeElementOffset,
      scrollToFn: elementScroll,
      ...options,
      onChange,
    });
    const measure = virtualizer.measureElement;
    virtualizer.measureElement = (node) => {
      measuring = true;
      try {
        measure(node);
      } finally {
        measuring = false;
      }
    };
    return { virtualizer, onChange };
  });
  const { virtualizer } = binding;
  virtualizer.setOptions({
    observeElementRect,
    observeElementOffset,
    scrollToFn: elementScroll,
    ...options,
    onChange: binding.onChange,
  });
  useLayoutEffect(() => virtualizer._didMount(), [virtualizer]);
  useLayoutEffect(() => {
    virtualizer._willUpdate();
  });
  return virtualizer;
}
