/**
 * SOURCE OF TRUTH KEYWORDS: stat-card barrel, StatCard export, StatUnit, StatFigure
 * WHAT:  Barrel for the stat-card folder.
 * WHY:   Callers import the folder, never the file, so the component can gain siblings without import churn.
 * WHERE: components/global/index.ts.
 */
export { StatCard, StatUnit, type StatCardProps, type StatCardSize } from "./StatCard";
export { StatFigure, type StatFigureProps } from "./StatFigure";
