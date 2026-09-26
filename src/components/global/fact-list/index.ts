/**
 * SOURCE OF TRUTH KEYWORDS: fact-list barrel, FactList export
 * WHAT:  Barrel for the fact-list folder.
 * WHY:   Other folders import `../fact-list`, never a file inside it (03 §3).
 * WHERE: components/global/index.ts, components/global/model-card.
 */
export { FactList, type Fact, type FactListProps } from "./FactList";
