/*!
 * SOURCE OF TRUTH KEYWORDS: chat completion request, llama-server request body, max_tokens estimate, enable_thinking false, no_think, parse chat answer, finish_reason, context budget
 * WHAT:  The llama-server chat request for one take (`chat_body`), its output budget (`max_tokens`), whether it fits
 *        the context at all (`fits_context`), a one-token warm-up request (`warm_up_body`) and reading the answer
 *        (`parse_answer`: the text and how generation ended).
 * WHY:   02 §8.3 stage 7 with 05 A12/A16: the exact system prompt, greedy sampling, a budget of about 1.5 × the
 *        input, stop sequences, and thinking switched off twice (the `/no_think` marker in the system prompt and
 *        `enable_thinking = false` for the chat template), since either alone has been seen to leak reasoning. The
 *        input is not tokenized first (a loopback round trip on the delivery path), so its tokens are estimated from
 *        characters with a divisor that errs high; an answer that hits the budget is rejected by the safety filter
 *        (`finish_reason: length`). A text too long for the context is refused before sending, so a long take falls
 *        back at once instead of waiting for the server's error. `cache_prompt` keeps the system prompt's tokens
 *        between takes, and the warm-up fills that cache right after the sidecar starts (the first take is then as
 *        fast as the next). Only the OpenAI-compatible fields llama-server documents are used; the JSON is built with
 *        serde_json values, never by string formatting, so any text is escaped correctly.
 * WHERE: LlamaServerPolisher::polish and the sidecar's warm-up (mod.rs, sidecar.rs); tests below.
 */

use serde_json::{Value, json};

use super::safety::Finish;
use crate::types::{AppError, LlmPolishProfile, PortError, PortResult, ThinkingControl};

/// The path of the chat endpoint.
pub(super) const CHAT_PATH: &str = "/v1/chat/completions";

/// Tokens the chat template and the system prompt take on top of the text (Qwen3's template adds about 20 around
/// a ~25-token prompt; rounded up).
const OVERHEAD_TOKENS: u32 = 96;

/// The input's token count as the profile estimates it (at least 1 for any text).
fn estimated_tokens(profile: &LlmPolishProfile, text: &str) -> u32 {
    let characters = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    characters.div_ceil(u32::from(profile.chars_per_token.max(1)))
}

/// How many tokens the answer may use: `max_tokens_percent` of the estimate, plus `min_tokens`.
pub(super) fn max_tokens(profile: &LlmPolishProfile, text: &str) -> u32 {
    let scaled = u64::from(estimated_tokens(profile, text)) * u64::from(profile.max_tokens_percent);
    let budget = u32::try_from(scaled.div_ceil(100)).unwrap_or(u32::MAX);
    budget.saturating_add(u32::from(profile.min_tokens))
}

/// Whether the prompt and its answer budget fit a context of `context_tokens`.
pub(super) fn fits_context(profile: &LlmPolishProfile, text: &str, context_tokens: u32) -> bool {
    estimated_tokens(profile, text)
        .saturating_add(max_tokens(profile, text))
        .saturating_add(OVERHEAD_TOKENS)
        <= context_tokens
}

/// The system message: the prompt, and the thinking switch-off marker when the model has one.
fn system_message(profile: &LlmPolishProfile) -> String {
    match profile.thinking {
        ThinkingControl::NotApplicable => profile.system_prompt.to_owned(),
        ThinkingControl::Disable { marker, .. } => format!("{} {marker}", profile.system_prompt),
    }
}

fn request(profile: &LlmPolishProfile, text: &str, max_tokens: u32) -> Vec<u8> {
    let mut body = json!({
        "messages": [
            { "role": "system", "content": system_message(profile) },
            { "role": "user", "content": text },
        ],
        "temperature": profile.temperature,
        "max_tokens": max_tokens,
        "stop": profile.stop,
        "stream": false,
        "cache_prompt": true,
    });
    if let (ThinkingControl::Disable { .. }, Some(fields)) =
        (profile.thinking, body.as_object_mut())
    {
        fields.insert(
            "chat_template_kwargs".to_owned(),
            json!({ "enable_thinking": false }),
        );
    }
    body.to_string().into_bytes()
}

/// The request that polishes `text`.
pub(super) fn chat_body(profile: &LlmPolishProfile, text: &str) -> Vec<u8> {
    request(profile, text, max_tokens(profile, text))
}

/// A one-token request that loads the system prompt into the prompt cache (no user text in it).
pub(super) fn warm_up_body(profile: &LlmPolishProfile) -> Vec<u8> {
    request(profile, "Hello.", 1)
}

/// The answer's text and how generation ended; anything else is `Polish`.
pub(super) fn parse_answer(body: &[u8]) -> PortResult<(String, Finish)> {
    let value: Value = serde_json::from_slice(body).map_err(|error| {
        PortError::new(AppError::Polish).with_detail(format!("the answer is not JSON: {error}"))
    })?;
    let choice = value
        .pointer("/choices/0")
        .ok_or_else(|| PortError::new(AppError::Polish).with_detail("the answer has no choice"))?;
    let text = choice
        .pointer("/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| PortError::new(AppError::Polish).with_detail("the answer has no text"))?;
    let finish = match choice.get("finish_reason").and_then(Value::as_str) {
        Some("length") => Finish::Length,
        _ => Finish::Complete,
    };
    Ok((text.to_owned(), finish))
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
        stop: &["<|im_start|>"],
        safety: LlmSafety {
            max_length_change_percent: 35,
            preambles: &["Here is"],
        },
    };

    #[test]
    fn the_body_carries_the_prompt_the_text_and_every_switch() {
        let text = "he said \"hi\" and\nleft";
        let body: Value = serde_json::from_slice(&chat_body(&PROFILE, text)).unwrap();
        assert_eq!(
            body["messages"][0]["content"],
            json!(
                "Fix grammar and punctuation. Keep meaning and wording. Output only the text. /no_think"
            )
        );
        assert_eq!(body["messages"][1]["content"], json!(text));
        assert_eq!(body["temperature"], json!(0.0));
        assert_eq!(body["stream"], json!(false));
        assert_eq!(body["stop"], json!(["<|im_start|>"]));
        assert_eq!(
            body["chat_template_kwargs"]["enable_thinking"],
            json!(false)
        );
        assert_eq!(body["max_tokens"], json!(max_tokens(&PROFILE, text)));
        let plain = LlmPolishProfile {
            thinking: ThinkingControl::NotApplicable,
            ..PROFILE
        };
        let body: Value = serde_json::from_slice(&chat_body(&plain, text)).unwrap();
        assert!(body.get("chat_template_kwargs").is_none());
        assert_eq!(body["messages"][0]["content"], json!(PROFILE.system_prompt));
        let warm: Value = serde_json::from_slice(&warm_up_body(&PROFILE)).unwrap();
        assert_eq!(warm["max_tokens"], json!(1));
    }

    #[test]
    fn the_budget_is_one_and_a_half_times_the_estimate_plus_a_floor() {
        // 30 characters ≈ 10 tokens → 15 + 16.
        assert_eq!(max_tokens(&PROFILE, &"a".repeat(30)), 31);
        assert_eq!(max_tokens(&PROFILE, ""), 16);
        assert!(fits_context(&PROFILE, &"a".repeat(3_000), 4_096));
        assert!(!fits_context(&PROFILE, &"a".repeat(6_000), 4_096));
    }

    #[test]
    fn answers_are_read_with_their_finish_reason() {
        let complete = br#"{"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Hi."}}]}"#;
        assert_eq!(
            parse_answer(complete).unwrap(),
            ("Hi.".to_owned(), Finish::Complete)
        );
        let cut = br#"{"choices":[{"finish_reason":"length","message":{"content":"Hi"}}]}"#;
        assert_eq!(parse_answer(cut).unwrap().1, Finish::Length);
        for broken in [
            &b"not json"[..],
            br#"{"choices":[]}"#,
            br#"{"choices":[{"message":{}}]}"#,
        ] {
            assert_eq!(
                parse_answer(broken).err().map(PortError::into_app_error),
                Some(AppError::Polish)
            );
        }
    }
}
