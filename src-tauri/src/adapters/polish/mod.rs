/*!
 * SOURCE OF TRUTH KEYWORDS: polish adapters, TextPolisher implementations, RulePolisher, text cleanup engines
 * WHAT:  Adapters behind the TextPolisher port.
 * WHY:   A polish stage is an engine: the registry builds the selected ones (registry/engines.rs) and the polish chain
 *        only sees `Arc<dyn TextPolisher>` and its caps (02 §8.3). The opt-in LLM polisher (llama.cpp sidecar) joins
 *        as a sibling folder with its own registry entry.
 * WHERE: Built by the `rules` registry entry.
 */

mod rules;

pub use rules::RulePolisher;
