/**
 * SOURCE OF TRUTH KEYWORDS: model-card barrel, ModelCard, modelStatusLook, transferSummary, languagesSummary
 * WHAT:  Barrel for the model card: the component and its copy helpers.
 * WHY:   Routes import through components/global; the helpers are exported for pages that describe a model
 *        without the card (a toast, onboarding copy).
 * WHERE: components/global/index.ts.
 */
export { ModelCard, type ModelCardProps } from "./ModelCard";
export {
  engineKindLabel,
  isDeterminate,
  languagesSummary,
  modelStatusLook,
  transferSummary,
  type ModelStatusLook,
} from "./model-look";
