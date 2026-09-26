/**
 * SOURCE OF TRUTH KEYWORDS: model-card barrel, ModelCard, ModelCardActions, modelActionPlan, modelStatusLook, transferSummary, languagesSummary
 * WHAT:  Barrel for the model card: the component, its action buttons and their plan, and its copy helpers.
 * WHY:   Routes import through components/global; the helpers are exported for pages that describe a model
 *        without the card (a toast, onboarding copy).
 * WHERE: components/global/index.ts.
 */
export { ModelCard, type ModelCardProps } from "./ModelCard";
export { ModelCardActions, type ModelCardActionsProps } from "./ModelCardActions";
export {
  ALL_SECONDARY_MODEL_ACTIONS,
  modelActionPlan,
  needsNetwork,
  type ModelActionPlan,
  type PrimaryModelAction,
  type SecondaryModelAction,
} from "./model-actions";
export {
  engineKindLabel,
  isDeterminate,
  languagesSummary,
  modelStatusLook,
  transferSummary,
  type ModelStatusLook,
} from "./model-look";
