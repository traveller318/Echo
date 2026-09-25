/*!
 * SOURCE OF TRUTH KEYWORDS: take row rules, empty take rule, measured columns, heard text columns, completed take columns, word count, TranscriptChange lists
 * WHAT:  The row writes that end a take, as pure functions: `measured` (duration, speech), `is_empty` (too little
 *        speech or no text, 05 A4), `heard` (raw text, detected language) and `completed` (done, final text, word
 *        count; None when polish left nothing).
 * WHY:   A live take (the state machine) and a History retry (pipeline/retry.rs) must store a take the same way,
 *        so an `empty` or a word count never depends on which path produced it. Final text is stored trimmed (the
 *        chain's trailing space is for pasting, not for History), and words are whitespace-separated runs, the
 *        same count the dashboard sums (02 §7.4).
 * WHERE: pipeline/session/transition.rs (AllSegmentsDone, Delivered) and pipeline/retry.rs.
 */

use crate::types::{CaptureSummary, Language, TranscriptChange, TranscriptStatus};

/// The measured columns of a take's audio.
pub fn measured(audio: &CaptureSummary) -> Vec<TranscriptChange> {
    vec![
        TranscriptChange::DurationMs(saturate_u32(audio.duration_ms)),
        TranscriptChange::SpeechMs(saturate_u32(audio.speech_ms)),
    ]
}

/// Too little speech or no text: nothing is delivered and the take is stored `empty` (05 A4).
pub fn is_empty(audio: &CaptureSummary, joined_text: &str, min_speech_ms: u64) -> bool {
    audio.speech_ms < min_speech_ms || joined_text.trim().is_empty()
}

/// The raw text and, when the engine reported one, the detected language.
pub fn heard(raw_text: &str, language: Option<&Language>) -> Vec<TranscriptChange> {
    let mut changes = vec![TranscriptChange::RawText(Some(raw_text.to_owned()))];
    if let Some(language) = language {
        changes.push(TranscriptChange::Language(Some(language.clone())));
    }
    changes
}

/// A take whose polished text is `final_text` is done: status, trimmed text and its word count. None when the
/// text is blank (polish can leave nothing, e.g. a filler-only take), which makes the take `empty`.
pub fn completed(final_text: &str) -> Option<Vec<TranscriptChange>> {
    let text = final_text.trim();
    if text.is_empty() {
        return None;
    }
    Some(vec![
        TranscriptChange::Status(TranscriptStatus::Done),
        TranscriptChange::FinalText(Some(text.to_owned())),
        TranscriptChange::WordCount(word_count(text)),
    ])
}

/// Whitespace-separated words in `text`.
pub fn word_count(text: &str) -> u32 {
    u32::try_from(text.split_whitespace().count()).unwrap_or(u32::MAX)
}

/// `value` as u32, saturating (49 days of milliseconds, far beyond any take).
pub fn saturate_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(duration_ms: u64, speech_ms: u64) -> CaptureSummary {
        CaptureSummary {
            duration_ms,
            speech_ms,
            ..CaptureSummary::default()
        }
    }

    #[test]
    fn measured_columns_saturate() {
        assert_eq!(
            measured(&audio(u64::MAX, 400)),
            [
                TranscriptChange::DurationMs(u32::MAX),
                TranscriptChange::SpeechMs(400)
            ]
        );
    }

    #[test]
    fn a_take_is_empty_below_the_speech_minimum_or_without_text() {
        assert!(is_empty(&audio(3_000, 249), "Hello.", 250));
        assert!(is_empty(&audio(3_000, 1_000), "   ", 250));
        assert!(!is_empty(&audio(3_000, 250), "Hello.", 250));
    }

    #[test]
    fn heard_sets_the_language_only_when_reported() {
        assert_eq!(
            heard("Hi.", None),
            [TranscriptChange::RawText(Some("Hi.".to_owned()))]
        );
        let german = Language::from_static("de");
        assert_eq!(
            heard("Hallo.", Some(&german)).last(),
            Some(&TranscriptChange::Language(Some(german)))
        );
    }

    #[test]
    fn completed_trims_and_counts_or_reports_nothing_left() {
        assert_eq!(
            completed("  Two words. ").unwrap(),
            [
                TranscriptChange::Status(TranscriptStatus::Done),
                TranscriptChange::FinalText(Some("Two words.".to_owned())),
                TranscriptChange::WordCount(2),
            ]
        );
        assert_eq!(completed(" \n "), None);
        assert_eq!(word_count("one  two\tthree\nfour"), 4);
    }
}
