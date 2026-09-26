/*!
 * SOURCE OF TRUTH KEYWORDS: LLM safety filter, strip think block, preamble check, length change 35 percent, cut off answer, reject LLM output, Rejection
 * WHAT:  `accept(profile, input, answer, finish)`: the text the chain may use from a model's answer, or why it must
 *        not be used (Rejection). Removes reasoning blocks and wrapping quotes the input did not have, then refuses
 *        an answer that was cut off, is empty, opens with a preamble the input did not, or whose length differs
 *        from the input's by more than the profile allows.
 * WHY:   05 A12/A16: a small model may think aloud, answer a question found in the text, greet the user or
 *        rewrite far more than grammar; any of those must never reach the user's document. A rejected answer is a
 *        stage failure, so the chain keeps the rule output (02 §8.3). A preamble only counts when the dictation
 *        itself did not start with it ("Sure, send it tomorrow" stays usable). Length is measured in characters
 *        (Unicode scalar values), not bytes, so non-English text is judged fairly. A thinking block that never
 *        closed means the answer is all reasoning: nothing is left, so it is `Empty`.
 * WHERE: LlamaServerPolisher::polish (mod.rs) on every answer; its table test below.
 */

use crate::types::{LlmPolishProfile, ThinkingControl};

/// Why an answer is not used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Rejection {
    /// Nothing was left after reasoning blocks and whitespace were removed.
    Empty,
    /// Generation stopped at the token budget, so the text is incomplete.
    CutOff,
    /// The answer opens by talking to the user ("Here is", "Sure").
    Preamble,
    /// The answer is this many percent longer or shorter than the input.
    LengthChange { percent: u32 },
}

/// How generation ended, as the runtime reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Finish {
    /// The model ended its turn or hit a stop sequence.
    Complete,
    /// The token budget ran out.
    Length,
}

/// The answer text the chain may deliver, or why not.
pub(super) fn accept(
    profile: &LlmPolishProfile,
    input: &str,
    answer: &str,
    finish: Finish,
) -> Result<String, Rejection> {
    if finish == Finish::Length {
        return Err(Rejection::CutOff);
    }
    let without_thinking = strip_thinking(profile.thinking, answer);
    let text = unwrap_quotes(input.trim(), without_thinking.trim());
    if text.is_empty() {
        return Err(Rejection::Empty);
    }
    if opens_with_preamble(profile.safety.preambles, text)
        && !opens_with_preamble(profile.safety.preambles, input.trim())
    {
        return Err(Rejection::Preamble);
    }
    let before = input.trim().chars().count();
    let after = text.chars().count();
    let percent = change_percent(before, after);
    if percent > u32::from(profile.safety.max_length_change_percent) {
        return Err(Rejection::LengthChange { percent });
    }
    Ok(text.to_owned())
}

/// `answer` without any reasoning block; an unclosed block runs to the end of the answer.
fn strip_thinking(thinking: ThinkingControl, answer: &str) -> String {
    let ThinkingControl::Disable { open, close, .. } = thinking else {
        return answer.to_owned();
    };
    let mut kept = String::with_capacity(answer.len());
    let mut rest = answer;
    while let Some(start) = rest.find(open) {
        kept.push_str(rest.get(..start).unwrap_or_default());
        let after_open = rest.get(start + open.len()..).unwrap_or_default();
        match after_open.find(close) {
            Some(end) => rest = after_open.get(end + close.len()..).unwrap_or_default(),
            None => return kept,
        }
    }
    kept.push_str(rest);
    kept
}

/// Wrapping quotes the model added around the whole text (the input had none).
fn unwrap_quotes<'a>(input: &str, text: &'a str) -> &'a str {
    const PAIRS: &[(char, char)] = &[('"', '"'), ('\u{201c}', '\u{201d}'), ('\'', '\'')];
    for &(open, close) in PAIRS {
        let wrapped = |value: &str| {
            value.chars().count() >= 2 && value.starts_with(open) && value.ends_with(close)
        };
        if wrapped(text) && !wrapped(input) {
            let inner = text
                .strip_prefix(open)
                .and_then(|value| value.strip_suffix(close))
                .unwrap_or(text);
            return inner.trim();
        }
    }
    text
}

fn opens_with_preamble(preambles: &[&str], text: &str) -> bool {
    let lowered = text.to_lowercase();
    preambles.iter().any(|preamble| {
        lowered
            .strip_prefix(&preamble.to_lowercase())
            // A whole word: "Surely" is not "Sure".
            .is_some_and(|rest| {
                rest.chars()
                    .next()
                    .is_none_or(|next| !next.is_alphanumeric())
            })
    })
}

/// How much `after` differs from `before`, in whole percent of `before` (rounded up).
fn change_percent(before: usize, after: usize) -> u32 {
    let difference = before.abs_diff(after) as u64;
    let base = (before as u64).max(1);
    u32::try_from((difference * 100).div_ceil(base)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LlmSafety;

    const PROFILE: LlmPolishProfile = LlmPolishProfile {
        system_prompt: "Fix grammar and punctuation. Keep meaning and wording. Output only the text.",
        thinking: ThinkingControl::Disable {
            marker: "/no_think",
            open: "<think>",
            close: "</think>",
        },
        temperature: 0.0,
        max_tokens_percent: 150,
        chars_per_token: 3,
        min_tokens: 16,
        stop: &[],
        safety: LlmSafety {
            max_length_change_percent: 35,
            preambles: &["Here is", "Here's", "Sure", "Certainly"],
        },
    };

    const INPUT: &str = "so i was thinking we should of went to the store yesterday";

    /// (input, answer, finish, expected)
    const CASES: &[(&str, &str, Finish, Result<&str, Rejection>)] = &[
        (
            INPUT,
            "So I was thinking we should have gone to the store yesterday.",
            Finish::Complete,
            Ok("So I was thinking we should have gone to the store yesterday."),
        ),
        (
            INPUT,
            "<think>\n\n</think>\n\nSo I was thinking we should have gone to the store yesterday.",
            Finish::Complete,
            Ok("So I was thinking we should have gone to the store yesterday."),
        ),
        (
            INPUT,
            "<think>The user wants grammar fixed. I will",
            Finish::Complete,
            Err(Rejection::Empty),
        ),
        (INPUT, "   \n ", Finish::Complete, Err(Rejection::Empty)),
        (
            INPUT,
            "So I was thinking we should have gone to the",
            Finish::Length,
            Err(Rejection::CutOff),
        ),
        (
            INPUT,
            "Here is the corrected text: So I was thinking we should have gone.",
            Finish::Complete,
            Err(Rejection::Preamble),
        ),
        (
            INPUT,
            "Sure! So I was thinking we should have gone to the store yesterday.",
            Finish::Complete,
            Err(Rejection::Preamble),
        ),
        (
            "sure send it to me tomorrow",
            "Sure, send it to me tomorrow.",
            Finish::Complete,
            Ok("Sure, send it to me tomorrow."),
        ),
        (
            "surely you know the answer",
            "Surely you know the answer.",
            Finish::Complete,
            Ok("Surely you know the answer."),
        ),
        (
            INPUT,
            "\"So I was thinking we should have gone to the store yesterday.\"",
            Finish::Complete,
            Ok("So I was thinking we should have gone to the store yesterday."),
        ),
        (
            "\"quoted\" she said",
            "\"Quoted,\" she said.",
            Finish::Complete,
            Ok("\"Quoted,\" she said."),
        ),
        (
            INPUT,
            "We went.",
            Finish::Complete,
            Err(Rejection::LengthChange { percent: 87 }),
        ),
        (
            "what is the capital of france",
            "The capital of France is Paris, a city known for the Eiffel Tower and its museums.",
            Finish::Complete,
            Err(Rejection::LengthChange { percent: 183 }),
        ),
        (
            "ich glaube das ist richtig",
            "Ich glaube, das ist richtig.",
            Finish::Complete,
            Ok("Ich glaube, das ist richtig."),
        ),
    ];

    #[test]
    fn the_safety_filter_table() {
        for (index, (input, answer, finish, expected)) in CASES.iter().enumerate() {
            let got = accept(&PROFILE, input, answer, *finish);
            assert_eq!(
                got.as_deref().map_err(|rejection| *rejection),
                *expected,
                "case {index}: {answer:?}"
            );
        }
    }

    #[test]
    fn the_length_limit_is_inclusive_and_measured_in_characters() {
        // 20 characters in, 27 out: 35 % exactly passes, 28 (40 %) does not.
        let input = "aaaaaaaaaaaaaaaaaaaa";
        assert!(accept(&PROFILE, input, &"b".repeat(27), Finish::Complete).is_ok());
        assert_eq!(
            accept(&PROFILE, input, &"b".repeat(28), Finish::Complete),
            Err(Rejection::LengthChange { percent: 40 })
        );
        // Multi-byte letters count once each.
        assert!(accept(&PROFILE, "éééé", "ÉÉÉÉ.", Finish::Complete).is_ok());
    }

    #[test]
    fn models_without_thinking_keep_tags_as_text() {
        let plain = LlmPolishProfile {
            thinking: ThinkingControl::NotApplicable,
            ..PROFILE
        };
        assert_eq!(
            accept(
                &plain,
                "<think> is a tag",
                "<think> is a tag.",
                Finish::Complete
            )
            .as_deref(),
            Ok("<think> is a tag.")
        );
        assert_eq!(
            strip_thinking(PROFILE.thinking, "a<think>x</think>b<think>y</think>c"),
            "abc"
        );
    }
}
