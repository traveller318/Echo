/*!
 * SOURCE OF TRUTH KEYWORDS: PolishContext, polish options, text cleanup context, dictionary, remove fillers, source casing, source punctuation
 * WHAT:  PolishContext: everything a text polisher may need to know about a take besides its text: the language,
 *        what the ASR output already carries (punctuation, casing) and the user's cleanup settings.
 * WHY:   The polish chain is an ordered list of TextPolisher stages (02 §8.3). Each stage reads the same context,
 *        so a new stage (or a new LLM runtime) is an adapter plus a registry entry with no new parameters. Casing
 *        and punctuation come from the active engine's caps, so the rule polisher skips work the engine already
 *        did without ever knowing which engine ran (02 §3.4).
 * WHERE: Built once per take by pipeline/polish.rs from settings and `AsrCaps`; passed to `TextPolisher::polish`
 *        (ports/polish.rs).
 */

use super::{Language, StaticList, TextPair};

/// Per-take input to every polisher stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolishContext {
    /// Language of the text, when known (detected by the engine or picked in Settings).
    pub language: Option<Language>,
    /// The ASR output already carries punctuation (`AsrCaps.punctuation`).
    pub punctuated: bool,
    /// The ASR output already carries sentence casing (`AsrCaps.casing`).
    pub cased: bool,
    /// `polish.remove_fillers`.
    pub remove_fillers: bool,
    /// `polish.dictionary`: case-insensitive, whole-word replacements.
    pub dictionary: StaticList<TextPair>,
}
