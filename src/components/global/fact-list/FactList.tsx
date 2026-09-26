/**
 * SOURCE OF TRUTH KEYWORDS: FactList, facts list, definition list, label value pairs, dl grid, tabular facts
 * WHAT:  A two-column list of facts (`label` on the left, `value` on the right) as a `dl`, in --text-callout; a fact
 *        marked `numeric` uses tabular figures.
 * WHY:   The model card (and any later facts surface) shows short label/value facts; one component keeps their
 *        layout, type size and number font identical (04 §3.6: numbers are tabular) and keeps the semantics a screen reader reads as pairs.
 *        Facts come in as data, so a surface decides which to show and the list only lays them out.
 * WHERE: components/global/model-card (ModelCard). Exported through components/global.
 */
import { Fragment, type ReactNode } from "react";
import { cn } from "@/lib/cn";
import { NUMERIC_CLASS } from "@/lib/format";

export interface Fact {
  readonly label: string;
  readonly value: ReactNode;
  /** The value is a number or size, set in tabular figures. */
  readonly numeric?: boolean;
}

export interface FactListProps {
  readonly facts: readonly Fact[];
  readonly className?: string;
}

export function FactList({ facts, className }: FactListProps) {
  return (
    <dl data-slot="fact-list" className={cn("grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-callout", className)}>
      {facts.map((fact) => (
        <Fragment key={fact.label}>
          <dt className="text-fg-secondary">{fact.label}</dt>
          <dd className={cn("text-fg", fact.numeric === true && NUMERIC_CLASS)}>{fact.value}</dd>
        </Fragment>
      ))}
    </dl>
  );
}
