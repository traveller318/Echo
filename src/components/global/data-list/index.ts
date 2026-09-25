/**
 * SOURCE OF TRUTH KEYWORDS: data-list barrel, DataList, DataListProps, DataListRowState, DataListSearchProps, nextActiveIndex
 * WHAT:  Barrel for the DataList folder: the list, its props and slot state, the search props and the navigation
 *        key table.
 * WHY:   Callers import from `@/components/global` only (03 §3); the folder can be reorganised freely.
 * WHERE: components/global/index.ts.
 */
export { DataList, type DataListProps, type DataListRowState } from "./DataList";
export type { DataListSearchProps } from "./DataListSearch";
export { clampIndex, isNavigationKey, nextActiveIndex, NAVIGATION_KEYS, type NavigationKey } from "./data-list-navigation";
