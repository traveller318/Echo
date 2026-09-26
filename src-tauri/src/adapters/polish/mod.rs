/*!
 * SOURCE OF TRUTH KEYWORDS: polish adapters, TextPolisher implementations, RulePolisher, LlamaServerPolisher, text cleanup engines
 * WHAT:  Adapters behind the TextPolisher port.
 * WHY:   A polish stage is an engine: the registry builds the selected ones (registry/engines.rs) and the polish chain
 *        only sees `Arc<dyn TextPolisher>` and its caps (02 §8.3). `rules/` is the always-on rule pass;
 *        `llama_server/` is the opt-in LLM stage (a llama.cpp sidecar), configured entirely by its registry entry.
 * WHERE: Built by the `rules` and `qwen3-1.7b` registry entries.
 */

mod llama_server;
mod rules;

pub use llama_server::LlamaServerPolisher;
pub use rules::RulePolisher;
