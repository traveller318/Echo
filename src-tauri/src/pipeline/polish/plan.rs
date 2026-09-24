/*!
 * SOURCE OF TRUTH KEYWORDS: polish_plan, polish_context, PolishPlan from settings, PolishContext from caps, chain order, opt-in LLM slot
 * WHAT:  `polish_plan` decides from the registry and settings which stages the chain runs, in the fixed order of
 *        02 §8.3 (the always-on polishers in registry order, then the model polisher `polish.llm_engine` selects
 *        while grammar polish is on), and whether a trailing space ends the text. `polish_context` builds the
 *        per-take PolishContext from settings, the engine's caps and the take's language.
 * WHY:   The plan names engines without building them, so the chain can compare plans and keep stages it already
 *        runs. The model stage is listed even when its engine is not registered or installed: the chain reports it
 *        as unavailable and keeps the rule output, which is exactly the fallback 02 §8.3 asks for, instead of the
 *        plan silently dropping what the user enabled. Casing and punctuation come from the caps of the engine that
 *        transcribed the take, so the rules never redo what the engine did (02 §3.4).
 * WHERE: The session actor builds a PolishChain from `polish_plan` (again after SettingsChanged) and a
 *        `polish_context` per take; tests.
 */

use crate::{
    registry,
    types::{AsrCaps, Language, PolishContext, PolishPlan, SettingsSnapshot, StaticList},
};

/// The stages to run for the current settings.
pub fn polish_plan(settings: &SettingsSnapshot) -> PolishPlan {
    let mut stages: Vec<_> = registry::engines::always_on_polishers()
        .map(|entry| entry.id.clone())
        .collect();
    if let Some(model_stage) = registry::settings::llm_polisher(settings)
        && !stages.contains(&model_stage)
    {
        stages.push(model_stage);
    }
    PolishPlan {
        stages,
        trailing_space: registry::settings::trailing_space(settings),
    }
}

/// The context every stage reads for a take transcribed by an engine with `caps`, in `language` when known.
pub fn polish_context(
    settings: &SettingsSnapshot,
    caps: &AsrCaps,
    language: Option<Language>,
) -> PolishContext {
    PolishContext {
        language,
        punctuated: caps.punctuation,
        cased: caps.casing,
        remove_fillers: registry::settings::remove_fillers(settings),
        dictionary: StaticList::from(registry::settings::dictionary(settings).to_vec()),
    }
}
