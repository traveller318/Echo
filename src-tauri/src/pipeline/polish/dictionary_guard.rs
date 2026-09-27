/*!
 * SOURCE OF TRUTH KEYWORDS: dictionary guard, keeps_dictionary_terms, model stage rewrote a term, DictionaryTermLost, whole-word count, protect dictionary spellings
 * WHAT:  `keeps_dictionary_terms(before, after, dictionary)`: false when `after` holds fewer whole-word copies of any
 *        dictionary spelling (a pair's `to`, exactly as the user typed it) than `before` did.
 * WHY:   The dictionary is applied by the rule stage, first in the chain (02 §8.3); a model stage after it (grammar
 *        polish) may "correct" a spelling the user chose ("BridgeMind" → "Bridge Mind", "TypeScript" → "Typescript").
 *        A dictionary swap must land on every take while the dictionary is on, so a model answer that loses one is
 *        not used and the chain keeps the rule output, where every swap is in place. Counting (not just presence)
 *        catches a rewrite of one copy among several. The count is whole-word with the same boundary rule as the
 *        dictionary stage (a spelling whose edge is a letter or digit cannot sit inside a word), so "C" in "Cat"
 *        is not a copy of the spelling "C". Case-sensitive on purpose: the user's casing is the point. A pair that
 *        removes its word (empty `to`) has no spelling to keep. Pure, so it is table-tested alone.
 * WHERE: PolishChain::run_stage (chain.rs), on the output of every stage that runs a model (`caps.needs_model`).
 */

use crate::types::TextPair;

/// Whether `after` keeps every dictionary spelling `before` held, as often as `before` held it.
pub(super) fn keeps_dictionary_terms(before: &str, after: &str, dictionary: &[TextPair]) -> bool {
    dictionary.iter().all(|pair| {
        let term = pair.to.trim();
        term.is_empty() || whole_word_count(after, term) >= whole_word_count(before, term)
    })
}

/// Non-overlapping whole-word occurrences of `term` in `text`.
fn whole_word_count(text: &str, term: &str) -> usize {
    let word_start = term.chars().next().is_some_and(char::is_alphanumeric);
    let word_end = term.chars().next_back().is_some_and(char::is_alphanumeric);
    let mut count = 0;
    let mut from = 0;
    while let Some(found) = text.get(from..).and_then(|rest| rest.find(term)) {
        let start = from + found;
        let end = start + term.len();
        let glued_before = word_start
            && text
                .get(..start)
                .and_then(|head| head.chars().next_back())
                .is_some_and(char::is_alphanumeric);
        let glued_after = word_end
            && text
                .get(end..)
                .and_then(|tail| tail.chars().next())
                .is_some_and(char::is_alphanumeric);
        if glued_before || glued_after {
            // Step one character on, so a copy starting inside this match is still found.
            from = start + term.chars().next().map_or(1, char::len_utf8);
        } else {
            count += 1;
            from = end;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::StaticStr;

    fn pairs(list: &[(&str, &str)]) -> Vec<TextPair> {
        list.iter()
            .map(|(from, to)| TextPair {
                from: StaticStr::from((*from).to_owned()),
                to: StaticStr::from((*to).to_owned()),
            })
            .collect()
    }

    #[test]
    fn a_model_answer_must_keep_every_dictionary_spelling() {
        let dictionary = pairs(&[
            ("bridge mind", "BridgeMind"),
            ("type script", "TypeScript"),
            ("uh huh", ""),
            ("c", "C"),
        ]);
        // (before, after, kept)
        let cases: &[(&str, &str, bool)] = &[
            ("i use BridgeMind daily", "I use BridgeMind daily.", true),
            ("i use BridgeMind daily", "I use Bridge Mind daily.", false),
            (
                "BridgeMind and BridgeMind",
                "BridgeMind and Bridgemind.",
                false,
            ),
            ("TypeScript is nice", "Typescript is nice.", false),
            ("no terms here", "No terms here.", true),
            ("the Cat sat", "The cat sat.", true),
            ("I write C daily", "I write c daily.", false),
            ("I write C daily", "I write C daily and C too.", true),
            ("", "Anything.", true),
        ];
        for (before, after, kept) in cases {
            assert_eq!(
                keeps_dictionary_terms(before, after, &dictionary),
                *kept,
                "{before:?} → {after:?}"
            );
        }
    }

    #[test]
    fn copies_are_whole_words() {
        assert_eq!(whole_word_count("Echo, Echoes, reEcho, Echo", "Echo"), 2);
        assert_eq!(whole_word_count("C++ and C++11", "C++"), 2);
        assert_eq!(whole_word_count("ÜberApp über ÜberApp", "ÜberApp"), 2);
        assert_eq!(whole_word_count("aaa", "aa"), 0);
        assert_eq!(whole_word_count("", "Echo"), 0);
    }
}
