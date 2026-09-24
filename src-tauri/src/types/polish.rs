/*!
 * SOURCE OF TRUTH KEYWORDS: PolishContext, PolishPlan, PolishPolicy, PolishOutcome, PolishFallback, PolishFallbackReason, polish chain result, slow stage timeout
 * WHAT:  The polish chain's shapes: PolishContext (everything a stage may know about a take besides its text),
 *        PolishPlan (which stages run, in order, and whether a trailing space ends the text), PolishPolicy (the
 *        timeout a slow stage gets) and PolishOutcome (the final text, the stages that shaped it and the stages
 *        whose output was not used, with why).
 * WHY:   The polish chain is an ordered list of TextPolisher stages (02 §8.3). Each stage reads the same context,
 *        so a new stage (or a new LLM runtime) is an adapter plus a registry entry with no new parameters. Casing
 *        and punctuation come from the active engine's caps, so the rule polisher skips work the engine already
 *        did without ever knowing which engine ran (02 §3.4). The plan is data (engine ids), so it is decided from
 *        settings and the registry without building anything, and a rebuilt chain can keep the stages it already
 *        has (a running LLM sidecar is not restarted by an unrelated settings change). The 2 s budget is pipeline
 *        policy like SegmentPolicy, not a setting, and tests shorten it. A stage that loses (timeout, error, empty
 *        output, could not be built) never blocks delivery: the previous stage's text is kept and the fallback is
 *        reported so the session can log it; `polisher_ids` is what `transcripts.polisher_ids` stores.
 * WHERE: Built by pipeline/polish (plan from registry + settings, context per take); PolishContext is passed to
 *        `TextPolisher::polish` (ports/polish.rs); PolishOutcome goes to the session actor, which stores the text and
 *        `polisher_ids` on the transcript row.
 */

use std::time::Duration;

use super::{AppError, EngineId, Language, StaticList, TextPair};

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

/// Which polish stages run, in the fixed order of 02 §8.3, and how the text ends.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolishPlan {
    /// Registry polisher ids: the always-on stages in registry order, then the opt-in model stage.
    pub stages: Vec<EngineId>,
    /// `output.trailing_space`: append one space after the last stage.
    pub trailing_space: bool,
}

/// How long the chain waits for a stage before keeping the previous text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolishPolicy {
    /// Hard timeout for a `LatencyClass::Slow` stage (02 §6.2: 2 s, then fall back to the rule output).
    pub slow_stage_timeout_ms: u32,
}

impl PolishPolicy {
    pub const DEFAULT: Self = Self {
        slow_stage_timeout_ms: 2_000,
    };

    pub fn slow_stage_timeout(self) -> Duration {
        Duration::from_millis(u64::from(self.slow_stage_timeout_ms))
    }
}

impl Default for PolishPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Why a stage's output was not used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolishFallbackReason {
    /// The stage could not be built (unregistered id, adapter construction failed).
    Unavailable(AppError),
    /// The stage returned an error.
    Failed(AppError),
    /// A slow stage did not answer within `PolishPolicy::slow_stage_timeout_ms`.
    TimedOut,
    /// A slow stage returned no text for non-empty input.
    EmptyOutput,
}

/// A stage whose output the chain did not use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolishFallback {
    pub engine_id: EngineId,
    pub reason: PolishFallbackReason,
}

/// What the polish chain produced for one take.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolishOutcome {
    /// The text to deliver (with the trailing space when the plan asks for one).
    pub text: String,
    /// The stages whose output is in `text`, in chain order (`transcripts.polisher_ids`).
    pub polisher_ids: Vec<EngineId>,
    /// The stages whose output was not used.
    pub fallbacks: Vec<PolishFallback>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_policy_gives_slow_stages_two_seconds() {
        assert_eq!(
            PolishPolicy::default().slow_stage_timeout(),
            Duration::from_secs(2)
        );
    }
}
