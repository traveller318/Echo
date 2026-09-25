/**
 * SOURCE OF TRUTH KEYWORDS: DataList, virtualized list, TanStack Virtual, keyboard navigation, roving tabindex, row slot, actions slot, empty slot, infinite scroll, onEndReached
 * WHAT:  A generic list that owns everything reusable about a long list: its search field (DataListSearch),
 *        virtualization (only the rows near the viewport exist in the DOM), keyboard navigation (ArrowUp/Down,
 *        Home/End, PageUp/Down, Enter activates), focus (one active row; roving tabindex) and paging (asks for more
 *        near the end). What a row shows (`row`), its actions (`actions`), the empty state (`empty`) and anything
 *        under the rows (`footer`) come in as slots; items are any `T` with a stable key.
 * WHY:   04 §6: History (100k+ rows) and the Dashboard's recent takes share one list, so the hard parts live once
 *        (root CLAUDE.md §7). Rows are measured after render, so a one-line and a two-line row both fit without a
 *        fixed height. The active row is the only tabbable row, so Tab enters the list once and moves on to that
 *        row's actions; the actions of every other row are `inert` (out of the tab order and unclickable), shown on
 *        hover, focus or when active (04 §5 "on hover or focus"), and always occupy their space so text never
 *        jumps. Hovering makes a row active, as in a menu, so its actions are reachable by pointer. Focus follows
 *        the active row only for keyboard moves, after the virtualizer has scrolled it into the DOM. A new search
 *        resets to the top. The list is `role="list"` with each row's position and the
 *        total count announced (virtualization hides the rest from the accessibility tree).
 * WHERE: routes/history (all takes); later routes/dashboard (recent takes). Exported through
 *        components/global/index.ts.
 */
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { cn } from "@/lib/cn";
import { LIST_OVERSCAN, LIST_ROW_ESTIMATE } from "@/styles/placement";
import { clampIndex, nextActiveIndex } from "./data-list-navigation";
import { DataListSearch, type DataListSearchProps } from "./DataListSearch";
import { useVirtualRows } from "./use-virtual-rows";

/** What a slot knows about the row it renders. */
export interface DataListRowState {
  readonly index: number;
  /** The row is the list's active row (keyboard position or hover). */
  readonly active: boolean;
}

export interface DataListProps<T> {
  readonly items: readonly T[];
  /** A stable, unique key per item (e.g. its id). */
  readonly getKey: (item: T) => string;
  /** Accessible name of the list. */
  readonly label: string;
  /** Row slot: what one item shows. It must hold no interactive element; put those in `actions`. */
  readonly row: (item: T, state: DataListRowState) => ReactNode;
  /** Actions slot: buttons for one item, shown on hover, focus or when the row is active. */
  readonly actions?: (item: T, state: DataListRowState) => ReactNode;
  /** Empty slot: shown when there are no items. */
  readonly empty: ReactNode;
  /** Enter on a row, or a click on it. */
  readonly onActivate?: (item: T) => void;
  /** The search field; omit for a list without one. */
  readonly search?: DataListSearchProps;
  /** More items exist after the last one. */
  readonly hasMore?: boolean;
  /** The next items are being loaded (`onEndReached` is not called again meanwhile). */
  readonly loadingMore?: boolean;
  /** Called when the viewport nears the last item while `hasMore`. */
  readonly onEndReached?: () => void;
  /** Footer slot: under the last row, inside the scroll area (a loading line, an error). */
  readonly footer?: ReactNode;
  readonly className?: string;
}

export function DataList<T>({
  items,
  getKey,
  label,
  row,
  actions,
  empty,
  onActivate,
  search,
  hasMore = false,
  loadingMore = false,
  onEndReached,
  footer,
  className,
}: DataListProps<T>) {
  // The virtualizer's answers change while its identity does not, so React Compiler must not memoize this component.
  "use no memo";
  const viewport = useRef<HTMLDivElement>(null);
  const rows = useRef(new Map<number, HTMLDivElement>());
  const focusRequest = useRef<number | null>(null);
  const [active, setActive] = useState(0);
  const count = items.length;
  const activeIndex = count === 0 ? -1 : clampIndex(active, count);

  const virtualizer = useVirtualRows({
    count,
    getScrollElement: () => viewport.current,
    estimateSize: () => LIST_ROW_ESTIMATE,
    overscan: LIST_OVERSCAN,
    getItemKey: (index) => {
      const item = items[index];
      return item === undefined ? index : getKey(item);
    },
  });
  const virtualRows = virtualizer.getVirtualItems();

  // A new search starts at the top: the active row resets while rendering, the scroll after it.
  const searchValue = search?.value;
  const [searched, setSearched] = useState(searchValue);
  if (searched !== searchValue) {
    setSearched(searchValue);
    setActive(0);
  }
  useEffect(() => {
    virtualizer.scrollToOffset(0);
  }, [searchValue, virtualizer]);

  // Keyboard moves focus the new active row once it is rendered (it may need a scroll first).
  useLayoutEffect(() => {
    const index = focusRequest.current;
    if (index === null) {
      return;
    }
    const element = rows.current.get(index);
    if (element !== undefined) {
      focusRequest.current = null;
      element.focus({ preventScroll: true });
    }
  });

  const lastRendered = virtualRows.at(-1)?.index ?? -1;
  useEffect(() => {
    if (hasMore && !loadingMore && onEndReached !== undefined && lastRendered >= count - 1 - LIST_OVERSCAN) {
      onEndReached();
    }
  }, [count, hasMore, lastRendered, loadingMore, onEndReached]);

  const moveTo = useCallback(
    (index: number) => {
      setActive(index);
      focusRequest.current = index;
      virtualizer.scrollToIndex(index, { align: "auto" });
    },
    [virtualizer],
  );

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) {
      return;
    }
    const pageSize = (viewport.current?.clientHeight ?? 0) / LIST_ROW_ESTIMATE;
    const next = nextActiveIndex(event.key, activeIndex, count, pageSize);
    if (next !== null) {
      event.preventDefault();
      moveTo(next);
      return;
    }
    const item = items[activeIndex];
    // Enter activates only from the row itself, never from one of its action buttons.
    if (event.key === "Enter" && item !== undefined && event.target === rows.current.get(activeIndex)) {
      event.preventDefault();
      onActivate?.(item);
    }
  };

  const registerRow = (index: number) => (element: HTMLDivElement | null) => {
    if (element === null) {
      rows.current.delete(index);
    } else {
      rows.current.set(index, element);
    }
  };

  return (
    <div data-slot="data-list" className={cn("flex min-h-0 flex-col gap-3", className)}>
      {search === undefined ? null : (
        <DataListSearch
          {...search}
          onLeave={() => {
            if (count > 0) {
              moveTo(activeIndex);
            }
          }}
        />
      )}
      <div
        ref={viewport}
        data-slot="data-list-viewport"
        className="min-h-0 flex-1 overflow-y-auto"
        onKeyDown={onKeyDown}
      >
        {count === 0 ? (
          empty
        ) : (
          <div
            role="list"
            aria-label={label}
            className="relative w-full"
            style={{ height: virtualizer.getTotalSize() }}
          >
            {virtualRows.map((virtualRow) => {
              const item = items[virtualRow.index];
              if (item === undefined) {
                return null;
              }
              const index = virtualRow.index;
              const state: DataListRowState = { index, active: index === activeIndex };
              return (
                <div
                  key={virtualRow.key}
                  role="listitem"
                  data-index={index}
                  data-active={state.active}
                  aria-posinset={index + 1}
                  aria-setsize={count}
                  ref={virtualizer.measureElement}
                  className="group absolute inset-x-0 top-0 flex items-center gap-2 rounded-sm p-1"
                  style={{ transform: `translateY(${String(virtualRow.start)}px)` }}
                  onPointerEnter={() => {
                    setActive(index);
                  }}
                  onFocus={() => {
                    setActive(index);
                  }}
                >
                  <div
                    ref={registerRow(index)}
                    data-slot="data-list-row"
                    tabIndex={state.active ? 0 : -1}
                    className={cn(
                      "min-w-0 flex-1 cursor-default rounded-sm px-3 py-2",
                      "transition-colors duration-(--duration-fast) ease-standard",
                      "group-data-[active=true]:bg-fill hover:bg-fill-hover",
                    )}
                    onClick={() => {
                      setActive(index);
                      onActivate?.(item);
                    }}
                  >
                    {row(item, state)}
                  </div>
                  {actions === undefined ? null : (
                    <div
                      data-slot="data-list-actions"
                      inert={!state.active}
                      className={cn(
                        "invisible flex shrink-0 items-center gap-1",
                        "group-focus-within:visible group-hover:visible group-data-[active=true]:visible",
                      )}
                    >
                      {actions(item, state)}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
        {footer}
      </div>
    </div>
  );
}
