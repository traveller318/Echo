/**
 * SOURCE OF TRUTH KEYWORDS: MetricCard, metric stat card, registry metric card, hero metric, metric help
 * WHAT:  One registry summary metric as a StatCard: its label, its value formatted by its unit (figures large,
 *        unit words small) and its help sentence, as the footer on the hero card and behind the info button on
 *        the others.
 * WHY:   Every summary metric renders the same way from its MetricSpec (label, unit, help), so a new metric needs
 *        no new component; a value that is null (nothing to compute from yet) reads as an em dash, never 0.
 *        The hero has room to say what its number means outright; smaller cards keep the sentence one tap away.
 * WHERE: routes/dashboard/index.tsx (hero, primary and secondary rows).
 */
import { StatCard, StatFigure, type StatCardSize } from "@/components/global";
import { formatMetricParts } from "@/lib/format";
import type { SummaryMetric } from "./dashboard-layout";

export interface MetricCardProps {
  readonly metric: SummaryMetric;
  readonly value: number | null;
  readonly size?: StatCardSize;
}

export function MetricCard({ metric, value, size = "default" }: MetricCardProps) {
  const { spec } = metric;
  const hero = size === "hero";
  return (
    <StatCard
      data-metric={spec.id}
      size={size}
      label={spec.label}
      value={<StatFigure parts={formatMetricParts(spec.unit, value)} />}
      {...(hero ? { footer: spec.help } : { help: spec.help })}
    />
  );
}
