/*!
 * SOURCE OF TRUTH KEYWORDS: RulePolisher, rule polish, rule stages, dictionary fillers repeats spacing casing, Instant polisher, always-on polish
 * WHAT:  RulePolisher: the always-on TextPolisher (latency class Instant, no model, any language) that runs the rule
 *        stages of 02 §8.3 in their fixed order: dictionary, fillers, repeats, spacing and punctuation, casing.
 * WHY:   Rule cleanup must cost microseconds (02 §6.2: < 5 ms for the whole chain), so it is plain Rust over one token
 *        list, one file per stage. Language-specific data lives in lexicon.rs, so a language is a data change. The
 *        stages read only the PolishContext: fillers only when `remove_fillers`, casing only when the engine does not
 *        case (`cased`). The compiled dictionary is cached and rebuilt only when the user's pairs change, because the
 *        context carries the pairs on every take. `polish` never fails: rules always produce text (possibly empty
 *        when the take held nothing but fillers).
 * WHERE: Built by the `rules` registry engine entry (registry/engines.rs); run first by the polish chain
 *        (pipeline/polish).
 */

mod casing;
mod dictionary;
mod fillers;
mod lexicon;
mod repeats;
mod spacing;
mod text;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use parking_lot::Mutex;

use self::{
    casing::apply_casing, dictionary::CompiledDictionary, fillers::remove_fillers,
    repeats::collapse_repeats, spacing::normalize_spacing,
};
use crate::{
    ports::TextPolisher,
    types::{
        BoxFuture, LanguageSupport, LatencyClass, PolishContext, PolisherCaps, PortResult, TextPair,
    },
};

/// The rule-based polisher.
#[derive(Default)]
pub struct RulePolisher {
    /// The dictionary compiled from the pairs of the last take.
    dictionary: Mutex<Option<Arc<CompiledDictionary>>>,
}

impl RulePolisher {
    pub const CAPS: PolisherCaps = PolisherCaps {
        latency_class: LatencyClass::Instant,
        languages: LanguageSupport::Any,
        needs_model: false,
    };

    pub fn new() -> Self {
        Self::default()
    }

    /// Runs every rule stage over `text`.
    pub fn apply(&self, text: &str, context: &PolishContext) -> String {
        let text = self.dictionary(&context.dictionary).apply(text);
        let mut tokens = text::tokenize(&text);
        let lexicon = lexicon::resolve(context.language.as_ref(), &tokens);
        if context.remove_fillers
            && let Some(lexicon) = lexicon
        {
            tokens = remove_fillers(&tokens, lexicon);
        }
        tokens = collapse_repeats(&tokens, lexicon);
        tokens = normalize_spacing(&tokens);
        if !context.cased {
            apply_casing(&mut tokens, lexicon);
        }
        text::render(&tokens)
    }

    /// The compiled form of `pairs`, reusing the last one when the pairs did not change.
    fn dictionary(&self, pairs: &[TextPair]) -> Arc<CompiledDictionary> {
        let mut cached = self.dictionary.lock();
        match cached.as_ref() {
            Some(dictionary) if dictionary.is_compiled_from(pairs) => Arc::clone(dictionary),
            _ => {
                let dictionary = Arc::new(CompiledDictionary::compile(pairs));
                *cached = Some(Arc::clone(&dictionary));
                dictionary
            }
        }
    }
}

impl TextPolisher for RulePolisher {
    fn caps(&self) -> PolisherCaps {
        Self::CAPS
    }

    fn prepare(&self) -> BoxFuture<'_, PortResult<()>> {
        Box::pin(async { Ok(()) })
    }

    fn polish<'a>(
        &'a self,
        text: &'a str,
        context: &'a PolishContext,
    ) -> BoxFuture<'a, PortResult<String>> {
        Box::pin(async move { Ok(self.apply(text, context)) })
    }
}
