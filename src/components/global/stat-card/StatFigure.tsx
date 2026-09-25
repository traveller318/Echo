/**
 * SOURCE OF TRUTH KEYWORDS: StatFigure, number parts, figures and units, StatCard value, formatMetricParts
 * WHAT:  Renders formatted NumberParts (figures and unit words, lib/format.ts) as a StatCard value: each figure
 *        as text, each unit word as a StatUnit, so `2 h 5 min` shows large numbers with small units.
 * WHY:   The split comes from the one formatter the rest of the UI uses (formatMetricParts), so the card shows
 *        exactly the text other surfaces show, only styled: parts and unit words are set off by real spaces, so
 *        the text content (and what a screen reader says) is the joined string. Keys are the part positions
 *        because a value's parts never reorder.
 * WHERE: The `value` slot of StatCard on the Dashboard. Exported through components/global/index.ts.
 */
import type { NumberPart } from "@/lib/format";
import { StatUnit } from "./StatCard";

export interface StatFigureProps {
  readonly parts: readonly NumberPart[];
}

export function StatFigure({ parts }: StatFigureProps) {
  return (
    <>
      {parts.map((part, index) => (
        <span key={`${String(index)}-${part.unit}`} data-slot="stat-figure-part">
          {index === 0 ? null : " "}
          {part.value}
          {part.unit === "" ? null : (
            <>
              {" "}
              <StatUnit>{part.unit}</StatUnit>
            </>
          )}
        </span>
      ))}
    </>
  );
}
