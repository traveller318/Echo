/**
 * SOURCE OF TRUTH KEYWORDS: glass-surface barrel, GlassSurface export
 * WHAT:  Barrel for the glass-surface folder.
 * WHY:   Callers import the folder, never the file, so the component can gain siblings without import churn.
 * WHERE: components/global/index.ts and components/ui (popover and modal parts).
 */
export { GlassSurface, type GlassSurfaceProps } from "./GlassSurface";
export { GLASS_SURFACE_VARIANTS, type GlassSurfaceVariant } from "./glass-surface-variants";
