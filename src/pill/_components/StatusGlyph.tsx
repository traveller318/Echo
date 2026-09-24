/**
 * SOURCE OF TRUTH KEYWORDS: StatusGlyph, pill check glyph, pill warning glyph, status not by colour alone
 * WHAT:  The pill's status icon: a check (`success`, --color-success) or a warning triangle (`warning`,
 *        --color-warning), with an accessible name when no text accompanies it.
 * WHY:   04 §7: status is never conveyed by colour alone, so each status has its own shape, and the Done state (a
 *        glyph with no text) names itself for assistive tech.
 * WHERE: src/pill/Pill.tsx (done, copied and error layouts).
 */
import { CheckIcon, TriangleAlertIcon } from "lucide-react";

export interface StatusGlyphProps {
  readonly status: "success" | "warning";
  /** The accessible name; omit when visible text says the same. */
  readonly label?: string;
}

export function StatusGlyph({ status, label }: StatusGlyphProps) {
  const Icon = status === "success" ? CheckIcon : TriangleAlertIcon;
  return (
    <span
      data-slot="status-glyph"
      data-status={status}
      role={label === undefined ? undefined : "img"}
      aria-label={label}
      aria-hidden={label === undefined ? true : undefined}
      className={status === "success" ? "inline-flex shrink-0 text-success" : "inline-flex shrink-0 text-warning"}
    >
      <Icon className="size-icon-sm" />
    </span>
  );
}
