/**
 * SOURCE OF TRUTH KEYWORDS: nav-icon barrel, NavIcon export
 * WHAT:  Barrel for the nav-icon folder.
 * WHY:   components/global/index.ts re-exports folders, never files inside them (03 §3).
 * WHERE: components/global/index.ts.
 */
export { NavIcon, type NavIconProps } from "./NavIcon";
