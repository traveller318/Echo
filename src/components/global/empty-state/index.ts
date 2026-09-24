/**
 * SOURCE OF TRUTH KEYWORDS: empty-state barrel, EmptyState export
 * WHAT:  Barrel for the empty-state folder.
 * WHY:   Callers import the folder, never the file, so the component can gain siblings without import churn.
 * WHERE: components/global/index.ts.
 */
export { EmptyState, type EmptyStateProps } from "./EmptyState";
