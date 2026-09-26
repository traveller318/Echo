/**
 * SOURCE OF TRUTH KEYWORDS: TranscriptStatusBadge, status badge, glyph plus text, take status
 * WHAT:  A Badge for a TranscriptStatus: its glyph (tinted by the variant) and its label.
 * WHY:   One badge for every place a take's status shows, with glyph and text so status is never colour alone
 *        (04 §7); the look comes from TRANSCRIPT_STATUS_LOOK.
 * WHERE: TranscriptRow. Exported through components/global/index.ts.
 */
import type { TranscriptStatus } from "@/bindings";
import { Badge } from "@/components/ui";
import { TRANSCRIPT_STATUS_LOOK } from "./transcript-status";

export interface TranscriptStatusBadgeProps {
  readonly status: TranscriptStatus;
  readonly className?: string;
}

export function TranscriptStatusBadge({ status, className }: TranscriptStatusBadgeProps) {
  const look = TRANSCRIPT_STATUS_LOOK[status];
  const Icon = look.icon;
  return (
    <Badge variant={look.variant} className={className}>
      <Icon aria-hidden="true" />
      {look.label}
    </Badge>
  );
}
