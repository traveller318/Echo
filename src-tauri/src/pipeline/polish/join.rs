/*!
 * SOURCE OF TRUTH KEYWORDS: join_segments, segment join, joined by index, sentence break across segments, A3 join rule
 * WHAT:  `join_segments` joins a take's segment texts (already in index order) into one text for the polish chain.
 * WHY:   Segments are cut in pauses of at least 600 ms, and each is transcribed on its own, so each may end with its
 *        own punctuation and the next may start in lowercase (05 A3). One space separates segments (blank ones are
 *        skipped). When the previous segment ends a sentence ("hello." + "world") the next one starts with a capital
 *        even if the engine lowercased it, because a long pause after a full stop is a sentence break; when it ends
 *        without one ("I went to the" + "store") the text simply continues. An ellipsis is a trailing thought, not a
 *        sentence end. Only a letter is ever raised, never lowered, so a name the engine capitalized stays. Stray
 *        spaces or stacked marks at the seam are left to the rule polisher's spacing stage.
 * WHERE: The session actor joins the SegmentDone texts of a take, in index order, before running the PolishChain.
 */

/// The segment texts joined into one text.
pub fn join_segments<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    let mut joined = String::new();
    for segment in segments {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        if joined.is_empty() {
            joined.push_str(segment);
            continue;
        }
        joined.push(' ');
        if ends_sentence(&joined) {
            let mut chars = segment.chars();
            if let Some(first) = chars.next() {
                joined.extend(first.to_uppercase());
                joined.push_str(chars.as_str());
            }
        } else {
            joined.push_str(segment);
        }
    }
    joined
}

/// The text so far (ending in the separator space) ends with a sentence-ending mark that is not an ellipsis.
fn ends_sentence(joined: &str) -> bool {
    let mut marks = joined.trim_end().chars().rev();
    match (marks.next(), marks.next()) {
        (Some('!' | '?'), _) => true,
        (Some('.'), second) => second != Some('.'),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_segments_with_one_space_and_sentence_breaks() {
        let cases: &[(&[&str], &str)] = &[
            (&["hello.", "world"], "hello. World"),
            (&["Hello.", "World."], "Hello. World."),
            (&["I went to the", "store"], "I went to the store"),
            (&["Is it?", "yes"], "Is it? Yes"),
            (&["Wait!", "\u{e9}mile"], "Wait! \u{c9}mile"),
            (&["I was thinking...", "maybe"], "I was thinking... maybe"),
            (&["  padded  ", "", "   ", "text "], "padded text"),
            (&["We met", "Anna"], "We met Anna"),
            (&["one.", "2 more"], "one. 2 more"),
            (&[], ""),
        ];
        for (segments, expected) in cases {
            assert_eq!(
                join_segments(segments.iter().copied()),
                *expected,
                "{segments:?}"
            );
        }
    }
}
