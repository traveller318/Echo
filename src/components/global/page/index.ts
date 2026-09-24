/**
 * SOURCE OF TRUTH KEYWORDS: page barrel, Page export
 * WHAT:  Barrel for the page folder.
 * WHY:   components/global/index.ts re-exports folders, never files inside them (03 §3).
 * WHERE: components/global/index.ts.
 */
export { Page, type PageProps } from "./Page";
