/**
 * SOURCE OF TRUTH KEYWORDS: transcript-row barrel, TranscriptRow, TranscriptStatusBadge, TRANSCRIPT_STATUS_LOOK
 * WHAT:  Barrel for the transcript-row folder: the take row, its status badge and the status look table.
 * WHY:   Callers import `@/components/global` only (03 §3).
 * WHERE: components/global/index.ts.
 */
export { TranscriptRow, type TranscriptRowProps } from "./TranscriptRow";
export { TranscriptStatusBadge, type TranscriptStatusBadgeProps } from "./TranscriptStatusBadge";
export {
  TRANSCRIPT_STATUS_LOOK,
  transcriptPlaceholder,
  type BadgeVariant,
  type TranscriptStatusLook,
} from "./transcript-status";
