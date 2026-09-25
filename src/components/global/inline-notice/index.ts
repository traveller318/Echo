/**
 * SOURCE OF TRUTH KEYWORDS: inline-notice barrel, InlineNotice
 * WHAT:  Barrel for InlineNotice.
 * WHY:   Routes import through components/global and never reach into a folder (03 §3).
 * WHERE: components/global/index.ts.
 */
export { InlineNotice, type InlineNoticeProps } from "./InlineNotice";
