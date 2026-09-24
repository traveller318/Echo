/**
 * SOURCE OF TRUTH KEYWORDS: progress-bar barrel, ProgressBar export
 * WHAT:  Barrel for the progress-bar folder.
 * WHY:   Callers import the folder, never the file, so the component can gain siblings without import churn.
 * WHERE: components/global/index.ts.
 */
export { progressFraction } from "./progress-fraction";
export { ProgressBar, type ProgressBarProps } from "./ProgressBar";
