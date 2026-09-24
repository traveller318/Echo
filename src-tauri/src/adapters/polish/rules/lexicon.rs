/*!
 * SOURCE OF TRUTH KEYWORDS: Lexicon, LEXICONS, filler words, edge fillers, keep repeats, abbreviations, language detection markers, resolve lexicon
 * WHAT:  The per-language data the rule stages use (fillers, clause-edge filler phrases, words whose doubling is
 *        grammatical, abbreviations, words always capitalized) and `resolve`, which picks the lexicon for a take:
 *        the language the context names, otherwise the one whose marker words the text uses most.
 * WHY:   Adding a language is a data change: a new LEXICONS row, no stage code (02 §8.3). Language-specific rules
 *        are dangerous in the wrong language: English fillers "um" and "er" are real words in German, Dutch,
 *        Danish and Portuguese, and "i" is a word in Italian and Polish. Parakeet auto-detects and reports no
 *        language, so the text itself decides: each row lists common words that are distinctive for its language
 *        (never shared with another row) and the best unique score wins. Rows for languages without cleanup data
 *        exist only to win that vote and switch the English rules off. A tie between languages turns every
 *        language rule off. Text with no marker at all (a short reply such as "Um, okay.") gets the first row,
 *        English, the language of Echo's filler list and UI; a language picked in Settings always wins over the vote.
 * WHERE: rules/mod.rs resolves once per take; fillers.rs, repeats.rs and casing.rs read the chosen row.
 */

use super::text::Token;
use crate::types::Language;

/// Cleanup data for one language. Every word is lowercase.
pub struct Lexicon {
    /// ISO 639-1 code, as `Language` spells it.
    pub language: &'static str,
    /// Common words distinctive for this language (in no other row), used to recognise it.
    pub markers: &'static [&'static str],
    /// Hesitation words removed wherever they appear.
    pub fillers: &'static [&'static str],
    /// Filler phrases removed only at a clause edge (start of text, after or before a comma or full stop).
    pub edge_fillers: &'static [&'static [&'static str]],
    /// Doubled words are collapsed ("the the" → "the").
    pub collapse_repeats: bool,
    /// Words whose doubling is grammatical or emphatic ("had had", "no no").
    pub keep_repeats: &'static [&'static str],
    /// Words also collapsed across a comma ("I, I think" → "I think"): short function words only.
    pub comma_repeats: &'static [&'static str],
    /// Abbreviations (without their dot) after which a full stop does not end the sentence.
    pub abbreviations: &'static [&'static str],
    /// Words always written with a capital first letter, also inside contractions ("i" → "I", "i'm" → "I'm").
    pub capitalized: &'static [&'static str],
}

impl Lexicon {
    /// A row that only recognises its language; none of the language-specific rules run for it.
    const fn markers_only(language: &'static str, markers: &'static [&'static str]) -> Self {
        Self {
            language,
            markers,
            fillers: &[],
            edge_fillers: &[],
            collapse_repeats: false,
            keep_repeats: &[],
            comma_repeats: &[],
            abbreviations: &[],
            capitalized: &[],
        }
    }

    pub fn is_filler(&self, folded: &str) -> bool {
        self.fillers.contains(&folded)
    }

    pub fn keeps_repeat(&self, folded: &str) -> bool {
        self.keep_repeats.contains(&folded)
    }

    pub fn collapses_across_comma(&self, folded: &str) -> bool {
        self.comma_repeats.contains(&folded)
    }

    pub fn is_abbreviation(&self, folded: &str) -> bool {
        self.abbreviations.contains(&folded)
    }

    /// The word, or the part of a contraction before its apostrophe, is always capitalized.
    pub fn is_capitalized(&self, folded: &str) -> bool {
        let stem = folded
            .split(super::text::is_apostrophe)
            .next()
            .unwrap_or(folded);
        self.capitalized.contains(&stem)
    }
}

/// Every lexicon; the first is the fallback for text with no marker word.
pub const LEXICONS: &[Lexicon] = &[
    Lexicon {
        language: "en",
        markers: &[
            "the", "and", "you", "that", "this", "with", "have", "are", "were", "what", "they",
            "not", "can", "would", "could", "should", "there", "about", "it", "it's", "i'm",
            "don't", "your", "because", "think", "know", "going", "want", "yeah", "been", "which",
            "thanks", "please", "hello", "sounds", "good",
        ],
        fillers: &["um", "umm", "uhm", "uh", "uhh", "er", "erm", "hmm", "hm"],
        edge_fillers: &[&["you", "know"]],
        collapse_repeats: true,
        keep_repeats: &[
            "had", "that", "no", "yes", "yeah", "very", "really", "so", "bye", "ha", "haha", "hey",
            "now", "okay", "ok", "right", "well", "please", "go", "come", "wait", "oh", "knock",
            "many", "much", "more", "far", "long", "again", "over", "round", "hear", "never", "do",
            "is", "blah", "chop", "tick", "tut", "boo", "bang", "bla", "la", "da", "ding", "pooh",
            "yum", "ta", "cha", "mm",
        ],
        comma_repeats: &[
            "i", "the", "a", "an", "and", "but", "or", "to", "of", "in", "on", "it", "we", "you",
            "they", "he", "she", "is", "was", "my", "if", "for", "with",
        ],
        abbreviations: &[
            "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "st", "vs", "approx", "dept", "est",
            "fig", "inc", "ltd", "co", "mt",
        ],
        capitalized: &["i"],
    },
    Lexicon::markers_only(
        "de",
        &[
            "und", "ist", "nicht", "ich", "der", "das", "dass", "ein", "eine", "zu", "auf", "auch",
            "sich", "wir", "aber", "sind", "habe", "haben", "wird", "noch", "schon", "jetzt",
            "heute", "bitte", "danke", "kann", "sehr",
        ],
    ),
    Lexicon::markers_only(
        "nl",
        &[
            "het", "een", "niet", "ik", "zijn", "dat", "wat", "maar", "ook", "hij", "wij", "deze",
            "heel", "heb", "mijn", "naar", "voor", "bedankt", "goed", "nog",
        ],
    ),
    Lexicon::markers_only(
        "da",
        &[
            "ikke", "jeg", "og", "hvad", "meget", "også", "være", "hvor", "godt",
        ],
    ),
    Lexicon::markers_only(
        "sv",
        &[
            "och", "inte", "jag", "är", "att", "mycket", "också", "vad", "tack", "bra",
        ],
    ),
    Lexicon::markers_only(
        "pt",
        &[
            "não", "você", "é", "uma", "isso", "muito", "também", "então", "ele", "ao", "obrigado",
            "obrigada", "estou",
        ],
    ),
    Lexicon::markers_only(
        "es",
        &[
            "el", "los", "las", "y", "muy", "pero", "también", "hay", "estoy", "eso", "yo",
            "usted", "ahora", "gracias", "bueno",
        ],
    ),
    Lexicon::markers_only(
        "fr",
        &[
            "les", "est", "une", "vous", "nous", "pas", "dans", "pour", "avec", "c'est", "très",
            "sont", "cette", "j'ai", "être", "merci", "oui",
        ],
    ),
    Lexicon::markers_only(
        "it",
        &[
            "che", "è", "sono", "gli", "della", "questo", "questa", "anche", "perché", "molto",
            "sì", "ho", "grazie",
        ],
    ),
    Lexicon::markers_only(
        "pl",
        &[
            "się",
            "jest",
            "że",
            "czy",
            "mnie",
            "już",
            "dlaczego",
            "bardzo",
            "jestem",
            "dziękuję",
            "tylko",
        ],
    ),
    Lexicon::markers_only(
        "cs",
        &[
            "není", "jsem", "velmi", "také", "protože", "když", "děkuji", "jsou", "tady", "jsme",
        ],
    ),
];

/**
 * SOURCE OF TRUTH KEYWORDS: resolve lexicon, language vote, marker score, language-specific rules switch
 * WHAT:  The lexicon for a take: the row of `language` when the context names one (None when no row exists, so
 *        only the universal rules run), otherwise the unique best marker score among `tokens`' words, otherwise
 *        (no marker at all) the first row.
 * WHY:   See the file header: a wrong language's rules would delete real words, so a tie runs none of them.
 * WHERE: RulePolisher::apply (rules/mod.rs), once per take after the dictionary stage.
 */
pub fn resolve(language: Option<&Language>, tokens: &[Token]) -> Option<&'static Lexicon> {
    if let Some(language) = language {
        return LEXICONS
            .iter()
            .find(|lexicon| lexicon.language == language.as_str());
    }
    let words: Vec<String> = tokens
        .iter()
        .filter(|token| token.is_word())
        .map(Token::folded)
        .collect();
    let scores: Vec<usize> = LEXICONS
        .iter()
        .map(|lexicon| {
            words
                .iter()
                .filter(|word| lexicon.markers.contains(&word.as_str()))
                .count()
        })
        .collect();
    let best = scores.iter().copied().max().unwrap_or(0);
    if best == 0 {
        return LEXICONS.first();
    }
    let mut leaders = scores
        .iter()
        .zip(LEXICONS)
        .filter(|(score, _)| **score == best);
    match (leaders.next(), leaders.next()) {
        (Some((_, lexicon)), None) => Some(lexicon),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{super::text::tokenize, *};

    fn detected(text: &str) -> Option<&'static str> {
        resolve(None, &tokenize(text)).map(|lexicon| lexicon.language)
    }

    #[test]
    fn every_row_is_lowercase_and_markers_belong_to_one_language() {
        let mut owners: HashMap<&str, &str> = HashMap::new();
        for lexicon in LEXICONS {
            let lists = [
                lexicon.markers,
                lexicon.fillers,
                lexicon.keep_repeats,
                lexicon.comma_repeats,
                lexicon.abbreviations,
                lexicon.capitalized,
            ];
            for word in lists.iter().flat_map(|list| list.iter()) {
                assert_eq!(*word, word.to_lowercase(), "{} {word}", lexicon.language);
            }
            for marker in lexicon.markers {
                if let Some(owner) = owners.insert(marker, lexicon.language) {
                    assert_eq!(owner, lexicon.language, "{marker} marks two languages");
                }
                assert!(
                    !lexicon.fillers.contains(marker),
                    "{marker} is both a marker and a filler"
                );
            }
        }
        let fillers: Vec<&str> = LEXICONS
            .iter()
            .flat_map(|lexicon| lexicon.fillers.iter().copied())
            .collect();
        for (marker, owner) in owners {
            assert!(
                !fillers.contains(&marker),
                "{owner} marker {marker} is another language's filler"
            );
        }
    }

    #[test]
    fn a_named_language_wins_over_the_vote() {
        let tokens = tokenize("the and you");
        let german = Language::from_static("de");
        assert_eq!(
            resolve(Some(&german), &tokens).map(|lexicon| lexicon.language),
            Some("de")
        );
        let japanese = Language::from_static("ja");
        assert!(resolve(Some(&japanese), &tokens).is_none());
    }

    #[test]
    fn the_text_votes_for_its_language() {
        assert_eq!(detected("I think that you are right"), Some("en"));
        assert_eq!(detected("Er ist um drei Uhr nicht da"), Some("de"));
        assert_eq!(detected("Er is niet veel"), Some("nl"));
        assert_eq!(detected("Um dia muito bom, obrigado"), Some("pt"));
        assert_eq!(detected("Io sono qui, grazie"), Some("it"));
    }

    #[test]
    fn no_marker_falls_back_to_english_and_a_tie_to_none() {
        assert_eq!(detected("Um, okay."), Some("en"));
        assert_eq!(detected(""), Some("en"));
        assert_eq!(
            detected("the und"),
            None,
            "one English and one German marker"
        );
    }

    #[test]
    fn contractions_share_the_capitalized_stem() {
        let english = &LEXICONS[0];
        assert!(english.is_capitalized("i"));
        assert!(english.is_capitalized("i'm"));
        assert!(english.is_capitalized("i\u{2019}ll"));
        assert!(!english.is_capitalized("it's"));
    }
}
