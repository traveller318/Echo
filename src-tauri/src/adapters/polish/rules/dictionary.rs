/*!
 * SOURCE OF TRUTH KEYWORDS: dictionary stage, CompiledDictionary, case-insensitive replace, whole-word replace, polish.dictionary, longest match
 * WHAT:  CompiledDictionary: the user's `polish.dictionary` pairs prepared for matching; `apply` replaces every
 *        case-insensitive, whole-word occurrence of a pair's `from` with its `to`, in one left-to-right pass.
 * WHY:   Stage 1 of 02 §8.3. Matching is on characters folded one-to-one to lowercase, so byte offsets never drift
 *        (a letter whose lowercase is several characters is compared as itself). Whole-word means a `from` that
 *        starts or ends with a letter or digit cannot start or end inside a word ("echo" never matches "echoes"),
 *        while a `from` with punctuation at its edge ("c++") still matches; an apostrophe is a boundary, so
 *        "claude" also fixes "claude's". Any whitespace in `from` matches any single whitespace character, so an
 *        entry does not depend on how the engine spaced it. The longest `from` wins at a position ("cloud code"
 *        before "cloud"), replaced text is never scanned again (no chained replacements), and `to` is written
 *        exactly as the user typed it. Entries are bucketed by their first folded character, so 500 pairs cost a
 *        handful of comparisons per position; compiling happens once per dictionary (rules/mod.rs caches it).
 * WHERE: RulePolisher::apply runs it first, before tokenizing (rules/mod.rs).
 */

use std::collections::{HashMap, HashSet};

use crate::types::TextPair;

struct Entry {
    from: Vec<char>,
    to: String,
    word_start: bool,
    word_end: bool,
}

/// Dictionary pairs ready for matching.
pub struct CompiledDictionary {
    source: Vec<TextPair>,
    entries: Vec<Entry>,
    /// Entry indices by first folded character, longest `from` first.
    by_first: HashMap<char, Vec<usize>>,
}

impl CompiledDictionary {
    /// Prepares `pairs`; a blank `from` is ignored and a repeated one keeps its first `to`.
    pub fn compile(pairs: &[TextPair]) -> Self {
        let mut entries: Vec<Entry> = Vec::with_capacity(pairs.len());
        let mut seen: HashSet<Vec<char>> = HashSet::with_capacity(pairs.len());
        for pair in pairs {
            let from: Vec<char> = pair.from.trim().chars().map(fold).collect();
            let (Some(first), Some(last)) = (from.first(), from.last()) else {
                continue;
            };
            if !seen.insert(from.clone()) {
                continue;
            }
            entries.push(Entry {
                word_start: first.is_alphanumeric(),
                word_end: last.is_alphanumeric(),
                from,
                to: pair.to.to_string(),
            });
        }
        let mut by_first: HashMap<char, Vec<usize>> = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            if let Some(first) = entry.from.first() {
                by_first.entry(*first).or_default().push(index);
            }
        }
        for bucket in by_first.values_mut() {
            bucket.sort_by_key(|index| std::cmp::Reverse(entries[*index].from.len()));
        }
        Self {
            source: pairs.to_vec(),
            entries,
            by_first,
        }
    }

    /// This dictionary was compiled from exactly `pairs`.
    pub fn is_compiled_from(&self, pairs: &[TextPair]) -> bool {
        self.source == pairs
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `text` with every whole-word match replaced.
    pub fn apply(&self, text: &str) -> String {
        if self.is_empty() {
            return text.to_owned();
        }
        let chars: Vec<char> = text.chars().collect();
        let folded: Vec<char> = chars.iter().copied().map(fold).collect();
        let mut out = String::with_capacity(text.len());
        let mut index = 0;
        while let Some(&c) = chars.get(index) {
            match self.match_at(&chars, &folded, index) {
                Some(entry) => {
                    out.push_str(&entry.to);
                    index += entry.from.len();
                }
                None => {
                    out.push(c);
                    index += 1;
                }
            }
        }
        out
    }

    /// The longest entry matching at `index`, respecting word boundaries.
    fn match_at(&self, chars: &[char], folded: &[char], index: usize) -> Option<&Entry> {
        let bucket = self.by_first.get(folded.get(index)?)?;
        let after_word = index
            .checked_sub(1)
            .and_then(|before| chars.get(before))
            .is_some_and(|c| c.is_alphanumeric());
        bucket
            .iter()
            .map(|entry| &self.entries[*entry])
            .find(|entry| {
                let end = index + entry.from.len();
                folded.get(index..end) == Some(entry.from.as_slice())
                    && !(entry.word_start && after_word)
                    && !(entry.word_end && chars.get(end).is_some_and(|c| c.is_alphanumeric()))
            })
    }
}

/// One-to-one lowercase fold; whitespace folds to a plain space.
fn fold(c: char) -> char {
    if c.is_whitespace() {
        return ' ';
    }
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(single), None) => single,
        _ => c,
    }
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

    /// Dictionary pairs, input, expected output.
    type Case = (
        &'static [(&'static str, &'static str)],
        &'static str,
        &'static str,
    );

    #[test]
    fn replaces_case_insensitive_whole_words() {
        let cases: &[Case] = &[
            (
                &[("cloud code", "Claude Code")],
                "I use cloud code daily.",
                "I use Claude Code daily.",
            ),
            (
                &[("cloud code", "Claude Code")],
                "Cloud Code is great",
                "Claude Code is great",
            ),
            (
                &[("cloud code", "Claude Code")],
                "CLOUD  CODE",
                "CLOUD  CODE",
            ),
            (
                &[("cloud code", "Claude Code")],
                "cloud\tcode",
                "Claude Code",
            ),
            (&[("echo", "Echo")], "echoes of echo", "echoes of Echo"),
            (&[("echo", "Echo")], "echo's window", "Echo's window"),
            (&[("echo", "Echo")], "re-echo", "re-Echo"),
            (&[("echo", "Echo")], "echo2", "echo2"),
            (&[("c++", "C++")], "I write c++ code", "I write C++ code"),
            (
                &[("cloud", "Cloud"), ("cloud code", "Claude Code")],
                "cloud code and cloud",
                "Claude Code and Cloud",
            ),
            (&[("gpt", "GPT"), ("gpt", "ignored")], "gpt", "GPT"),
            (&[("  ", "x"), ("", "y")], "a  b", "a  b"),
            (&[("über", "Über")], "ÜBER alles", "Über alles"),
            (&[("teh", "the")], "teh", "the"),
            (&[("remove me", "")], "please remove me now", "please  now"),
        ];
        for (list, input, expected) in cases {
            let dictionary = CompiledDictionary::compile(&pairs(list));
            assert_eq!(dictionary.apply(input), *expected, "{input}");
        }
    }

    #[test]
    fn replaced_text_is_not_scanned_again() {
        let dictionary = CompiledDictionary::compile(&pairs(&[("a", "b"), ("b", "c")]));
        assert_eq!(dictionary.apply("a b"), "b c");
    }

    #[test]
    fn remembers_its_source() {
        let source = pairs(&[("x", "y")]);
        let dictionary = CompiledDictionary::compile(&source);
        assert!(dictionary.is_compiled_from(&source));
        assert!(!dictionary.is_compiled_from(&[]));
        assert!(CompiledDictionary::compile(&[]).is_empty());
    }
}
