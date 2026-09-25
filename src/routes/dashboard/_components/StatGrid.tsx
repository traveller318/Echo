/**
 * SOURCE OF TRUTH KEYWORDS: StatGrid, stat card grid, dashboard row, equal columns
 * WHAT:  A row of equal-width columns, one per child card (`columns`), with the dashboard's card gap.
 * WHY:   The number of cards per row comes from the registry (how many metrics carry an emphasis), so the column
 *        count cannot be a fixed class; it is passed as a local custom property that one grid rule reads, which
 *        keeps every length a token (04 §3.5). `minmax(0, 1fr)` lets long numbers truncate instead of widening a
 *        column.
 * WHERE: routes/dashboard/index.tsx (primary and secondary metric rows).
 */
import type { CSSProperties, ReactNode } from "react";
import { cn } from "@/lib/cn";

type GridStyle = CSSProperties & Record<"--echo-columns", number>;

export interface StatGridProps {
  readonly columns: number;
  readonly children: ReactNode;
  readonly className?: string;
}

export function StatGrid({ columns, children, className }: StatGridProps) {
  const style: GridStyle = { "--echo-columns": Math.max(1, columns) };
  return (
    <div
      data-slot="stat-grid"
      style={style}
      className={cn("grid grid-cols-[repeat(var(--echo-columns),minmax(0,1fr))] gap-4", className)}
    >
      {children}
    </div>
  );
}
