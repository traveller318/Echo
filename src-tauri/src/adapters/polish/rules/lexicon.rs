/*!
 * SOURCE OF TRUTH KEYWORDS: Lexicon, LEXICONS, filler words, edge fillers, keep repeats, abbreviations, number words, language detection markers, resolve lexicon
 * WHAT:  The per-language data the rule stages use (fillers, clause-edge filler phrases, words whose doubling is
 *        grammatical, abbreviations, words always capitalized, spoken number words) and `resolve`, which picks the
 *        lexicon for a take:
 *        the language the context names, otherwise the one whose marker words the text uses most.
 * WHY:   Adding a language is a data change: a new LEXICONS row, no stage code (02 §8.3). Language-specific rules
 *        are dangerous in the wrong language: English fillers "um" and "er" are real words in German, Dutch,
 *        Danish and Portuguese, and "i" is a word in Italian and Polish. Parakeet auto-detects and reports no
 *        language, so the text itself decides: each row lists common words that are distinctive for its language
 *        (never shared with another row) and the best unique score wins. Rows for languages without cleanup data
 *        exist only to win that vote and switch the English rules off. A tie between languages turns every
 *        language rule off. Text with no marker at all (a short reply such as "Um, okay.") gets the first row,
 *        English, the language of Echo's filler list and UI; a language picked in Settings always wins over the vote.
 * WHERE: rules/mod.rs resolves once per take; fillers.rs, repeats.rs, numbers.rs and casing.rs read the chosen row.
 */

use std::ops::RangeInclusive;

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
    /// Spoken number words written as digits; `None` leaves numbers as the engine wrote them.
    pub numbers: Option<&'static NumberWords>,
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
            numbers: None,
        }
    }

    pub fn is_filler(&self, folded: &str) -> bool {
        self.fillers.contains(&folded)
    }

    /// A doubled number word is data, not a stutter ("twenty twenty" is a year).
    pub fn keeps_repeat(&self, folded: &str) -> bool {
        self.keep_repeats.contains(&folded)
            || self
                .numbers
                .is_some_and(|numbers| numbers.is_number_word(folded))
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
        numbers: Some(&ENGLISH_NUMBERS),
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
 * SOURCE OF TRUTH KEYWORDS: NumberWords, ENGLISH_NUMBERS, spoken number words, cardinal words, ordinal words, number scales, digits threshold, clock time, spoken year
 * WHAT:  One language's spoken number vocabulary and writing conventions, read by the numbers stage (numbers.rs):
 *        cardinal and ordinal words with their values, the joiner, decimal point and percent words, and how digits
 *        are written (group separator, decimal separator, clock separator, which lone words stay words).
 * WHY:   Speech engines write "twenty five" where a typist writes "25"; the conversion is language data, so another
 *        language is a new NumberWords value, not new stage code. A lone word below `digits_from` stays a word
 *        (the style-guide convention, and it keeps "one of them" and "no one" intact). Cardinal values classify
 *        themselves: below 10 a digit, below 20 a teen, below 100 a tens word, exactly 100 the hundred, above it a
 *        power-of-1000 scale. An ordinal carries the suffix written after its digits, because in English the last
 *        word alone decides it ("twenty first" → "21st").
 * WHERE: `Lexicon.numbers`; read by numbers.rs, and by `Lexicon::keeps_repeat` so repeats.rs never collapses
 *        "twenty twenty".
 */
pub struct NumberWords {
    /// Cardinal words with their values: 0–19, the tens 20–90, the hundred, then the scales (1 000, 1 000 000 …).
    pub cardinals: &'static [(&'static str, u64)],
    /// Ordinal words with their values and the suffix written after the digits.
    pub ordinals: &'static [(&'static str, u64, &'static str)],
    /// Ordinals that are also common nouns ("a twenty second clip"): a phrase ending in one stays in words.
    pub ambiguous_ordinals: &'static [&'static str],
    /// Words for a zero digit inside a clock time, a year or decimals ("ten oh five", "nineteen oh five").
    pub zero_digits: &'static [&'static str],
    /// Articles that count as 1 before the hundred or a scale ("a hundred and ten").
    pub articles: &'static [&'static str],
    /// The word joining the hundred or a scale to the rest ("one hundred and five").
    pub joiner: &'static str,
    /// The word that starts decimal digits ("three point five").
    pub decimal_point: &'static str,
    /// The word written as `percent_sign` after a number.
    pub percent: &'static str,
    pub percent_sign: char,
    /// A number said as one word below this stays a word ("five", "one"); anything longer is written in digits.
    pub digits_from: u64,
    /// A round multiple of a scale at least this large keeps the scale word ("2 million", "3.5 billion").
    pub named_scale_from: u64,
    /// Whole numbers from this value are grouped ("10,000"); smaller ones are not ("1523", years).
    pub group_from: u64,
    pub group_separator: char,
    pub decimal_separator: char,
    /// Two one-word numbers read as a clock time when the first is an hour here and the second at most 59.
    pub clock_hours: RangeInclusive<u64>,
    pub time_separator: char,
    /// Otherwise as a year when the first is a century here ("nineteen eighty four" → "1984").
    pub year_centuries: RangeInclusive<u64>,
}

impl NumberWords {
    /// The value of a cardinal word.
    pub fn cardinal(&self, folded: &str) -> Option<u64> {
        self.cardinals
            .iter()
            .find(|(word, _)| *word == folded)
            .map(|(_, value)| *value)
    }

    /// The value of an ordinal word and the suffix written after its digits.
    pub fn ordinal(&self, folded: &str) -> Option<(u64, &'static str)> {
        self.ordinals
            .iter()
            .find(|(word, ..)| *word == folded)
            .map(|(_, value, suffix)| (*value, *suffix))
    }

    pub fn is_number_word(&self, folded: &str) -> bool {
        self.cardinal(folded).is_some() || self.ordinal(folded).is_some()
    }
}

/// English number words.
const ENGLISH_NUMBERS: NumberWords = NumberWords {
    cardinals: &[
        ("zero", 0),
        ("one", 1),
        ("two", 2),
        ("three", 3),
        ("four", 4),
        ("five", 5),
        ("six", 6),
        ("seven", 7),
        ("eight", 8),
        ("nine", 9),
        ("ten", 10),
        ("eleven", 11),
        ("twelve", 12),
        ("thirteen", 13),
        ("fourteen", 14),
        ("fifteen", 15),
        ("sixteen", 16),
        ("seventeen", 17),
        ("eighteen", 18),
        ("nineteen", 19),
        ("twenty", 20),
        ("thirty", 30),
        ("forty", 40),
        ("fifty", 50),
        ("sixty", 60),
        ("seventy", 70),
        ("eighty", 80),
        ("ninety", 90),
        ("hundred", 100),
        ("thousand", 1_000),
        ("million", 1_000_000),
        ("billion", 1_000_000_000),
        ("trillion", 1_000_000_000_000),
    ],
    ordinals: &[
        ("first", 1, "st"),
        ("second", 2, "nd"),
        ("third", 3, "rd"),
        ("fourth", 4, "th"),
        ("fifth", 5, "th"),
        ("sixth", 6, "th"),
        ("seventh", 7, "th"),
        ("eighth", 8, "th"),
        ("ninth", 9, "th"),
        ("tenth", 10, "th"),
        ("eleventh", 11, "th"),
        ("twelfth", 12, "th"),
        ("thirteenth", 13, "th"),
        ("fourteenth", 14, "th"),
        ("fifteenth", 15, "th"),
        ("sixteenth", 16, "th"),
        ("seventeenth", 17, "th"),
        ("eighteenth", 18, "th"),
        ("nineteenth", 19, "th"),
        ("twentieth", 20, "th"),
        ("thirtieth", 30, "th"),
        ("fortieth", 40, "th"),
        ("fiftieth", 50, "th"),
        ("sixtieth", 60, "th"),
        ("seventieth", 70, "th"),
        ("eightieth", 80, "th"),
        ("ninetieth", 90, "th"),
        ("hundredth", 100, "th"),
        ("thousandth", 1_000, "th"),
        ("millionth", 1_000_000, "th"),
        ("billionth", 1_000_000_000, "th"),
        ("trillionth", 1_000_000_000_000, "th"),
    ],
    ambiguous_ordinals: &["second"],
    zero_digits: &["oh", "o"],
    articles: &["a"],
    joiner: "and",
    decimal_point: "point",
    percent: "percent",
    percent_sign: '%',
    digits_from: 10,
    named_scale_from: 1_000_000,
    group_from: 10_000,
    group_separator: ',',
    decimal_separator: '.',
    clock_hours: 1..=12,
    time_separator: ':',
    year_centuries: 10..=29,
};

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
            let number_words = lexicon.numbers.into_iter().flat_map(|numbers| {
                numbers
                    .cardinals
                    .iter()
                    .map(|(word, _)| *word)
                    .chain(numbers.ordinals.iter().map(|(word, ..)| *word))
                    .chain(numbers.ambiguous_ordinals.iter().copied())
                    .chain(numbers.zero_digits.iter().copied())
                    .chain(numbers.articles.iter().copied())
                    .chain([numbers.joiner, numbers.decimal_point, numbers.percent])
            });
            for word in lists
                .iter()
                .flat_map(|list| list.iter().copied())
                .chain(number_words)
            {
                assert_eq!(word, word.to_lowercase(), "{} {word}", lexicon.language);
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
    fn number_values_follow_their_classes() {
        for numbers in LEXICONS.iter().filter_map(|lexicon| lexicon.numbers) {
            let values = numbers
                .cardinals
                .iter()
                .map(|(_, value)| *value)
                .chain(numbers.ordinals.iter().map(|(_, value, _)| *value));
            for value in values {
                let power_of_1000 = value >= 1_000
                    && value.ilog10().is_multiple_of(3)
                    && 10u64.pow(value.ilog10()) == value;
                let classified = value < 20
                    || (value < 100 && value.is_multiple_of(10))
                    || value == 100
                    || power_of_1000;
                assert!(
                    classified,
                    "{value} is no digit, teen, tens, hundred or scale"
                );
            }
            for ordinal in numbers.ambiguous_ordinals {
                assert!(
                    numbers.ordinal(ordinal).is_some(),
                    "{ordinal} is no ordinal"
                );
            }
        }
    }

    #[test]
    fn doubled_number_words_are_kept() {
        let english = &LEXICONS[0];
        assert!(english.keeps_repeat("twenty"));
        assert!(english.keeps_repeat("fifth"));
        assert!(!english.keeps_repeat("the"));
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
