/*!
 * SOURCE OF TRUTH KEYWORDS: numbers stage, spoken numbers to digits, write_numbers, number words, inverse text normalization, clock time, spoken year, ordinal suffix, percent, decimal point
 * WHAT:  `write_numbers` rewrites spoken number phrases in digits with the lexicon's NumberWords: "twenty five" →
 *        "25", "one thousand five hundred twenty three" and "fifteen twenty three" → "1523", "ten thirty" →
 *        "10:30", "three point five" → "3.5", "fifty percent" → "50%", "twenty first" → "21st", "two million" →
 *        "2 million", "ten thousand" → "10,000".
 * WHY:   Stage 4 of 02 §8.3. Speech engines (Parakeet) spell numbers out; a typist writes digits. Only a whole
 *        phrase the reader understands is rewritten, anything else stays as spoken. A lone word below `digits_from`
 *        stays a word ("one of them", "five apples"), a phrase ending in an ambiguous ordinal stays ("a twenty
 *        second clip"), and an article with one scale word stays ("a hundred people"), because only a longer phrase
 *        proves a number was dictated. Words join across one space (never a line break or a comma, so "twenty,
 *        fifty" stays two numbers) and a tens word joins its unit across a hyphen ("twenty-five"). Two one-word
 *        numbers in a row are a clock time or a year, never a sum. The digits are re-tokenized, so later stages
 *        see "3.5" and "10:30" exactly as if the engine had written them. Values use checked arithmetic: a phrase
 *        too large for u64 stays in words.
 * WHERE: RulePolisher::apply (rules/mod.rs), after repeats (which keep doubled number words) and before spacing
 *        and casing, when the take's lexicon has NumberWords.
 */

use super::{
    lexicon::NumberWords,
    text::{Token, tokenize},
};

/// What a number word does in a phrase, decided by its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Term {
    /// 0–9.
    Digit(u64),
    /// 10–19.
    Teen(u64),
    /// 20, 30 … 90; takes a following digit ("twenty five").
    Tens(u64),
    /// 100: multiplies the group before it.
    Hundred,
    /// 1 000, 1 000 000 …: multiplies the group before it.
    Scale(u64),
}

impl Term {
    const fn of(value: u64) -> Self {
        match value {
            0..=9 => Self::Digit(value),
            10..=19 => Self::Teen(value),
            20..=99 => Self::Tens(value),
            100 => Self::Hundred,
            _ => Self::Scale(value),
        }
    }
}

/// A number word: its term and, for an ordinal, the suffix written after the digits.
#[derive(Debug, Clone, Copy)]
struct Word {
    term: Term,
    ordinal: Option<&'static str>,
}

/// A number below 1000 read from consecutive words.
#[derive(Debug, Clone, Copy)]
struct Group {
    value: u64,
    /// Index of its last token.
    last: usize,
    words: usize,
    ordinal: Option<&'static str>,
}

/// A whole spoken number phrase.
#[derive(Debug, Default)]
struct Number {
    /// The whole part; with a kept scale word, only the part before it ("2" of "2 million").
    whole: u64,
    /// Decimal digits after the point.
    fraction: String,
    /// Index of the scale word written after the digits.
    scale_word: Option<usize>,
    ordinal: Option<&'static str>,
    percent: bool,
    /// Words read, the article included.
    words: usize,
    /// The phrase opens with an article standing for 1 ("a hundred and ten").
    article: bool,
    /// Index of its last token.
    last: usize,
}

/// `tokens` with every spoken number phrase written in digits.
pub fn write_numbers(tokens: &[Token], numbers: &NumberWords) -> Vec<Token> {
    let reader = Reader { tokens, numbers };
    let mut out = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        if let Some((written, last)) = reader.digits_at(index) {
            out.extend(tokenize(&written));
            index = last + 1;
        } else {
            out.push(tokens[index].clone());
            index += 1;
        }
    }
    out
}

/// Reads number phrases from the tokens.
struct Reader<'a> {
    tokens: &'a [Token],
    numbers: &'a NumberWords,
}

impl Reader<'_> {
    /// The digits for the phrase starting at `index`, and the index of its last token.
    fn digits_at(&self, index: usize) -> Option<(String, usize)> {
        let number = self.number(index)?;
        self.clock_or_year(&number).or_else(|| {
            self.worth_digits(&number)
                .then(|| (self.write(&number), number.last))
        })
    }

    /// The longest number phrase starting at `index`.
    fn number(&self, index: usize) -> Option<Number> {
        let (head, article) = match self.article(index) {
            Some(head) => (head, true),
            None => (self.below_100(index)?, false),
        };
        let mut group = self.hundreds(head);
        let mut number = Number {
            words: group.words,
            article,
            ..Number::default()
        };
        let mut larger = u64::MAX;
        let mut scales = 0;
        loop {
            number.last = group.last;
            if group.ordinal.is_some() {
                number.whole = number.whole.checked_add(group.value)?;
                number.ordinal = group.ordinal;
                return Some(number);
            }
            let Some((at, scale, word)) = self
                .scale_after(group.last)
                .filter(|(_, scale, _)| *scale < larger)
            else {
                number.whole = number.whole.checked_add(group.value)?;
                break;
            };
            number.whole = number.whole.checked_add(group.value.checked_mul(scale)?)?;
            number.words += 1;
            number.last = at;
            larger = scale;
            scales += 1;
            if word.ordinal.is_some() {
                number.ordinal = word.ordinal;
                return Some(number);
            }
            let below_1000 = |start| {
                Some(self.hundreds(self.below_100(start)?)).filter(|next| next.value < 1_000)
            };
            match self.addend(at, below_1000) {
                Some((next, joined)) => {
                    number.words += joined + next.words;
                    group = next;
                }
                None => {
                    // "two million" keeps its scale word; "two million five" does not.
                    if scales == 1 && scale >= self.numbers.named_scale_from {
                        number.whole = group.value;
                        number.scale_word = Some(at);
                    }
                    return Some(self.with_percent(number));
                }
            }
        }
        if scales == 0
            && let Some((fraction, last, words)) = self.fraction(number.last)
        {
            number.fraction = fraction;
            number.last = last;
            number.words += words;
            if let Some((at, _, word)) = self.scale_after(last)
                && word.ordinal.is_none()
            {
                number.scale_word = Some(at);
                number.last = at;
                number.words += 1;
                return Some(number);
            }
        }
        Some(self.with_percent(number))
    }

    /// An article standing for 1 when the hundred or a scale follows it ("a hundred", "a million").
    fn article(&self, index: usize) -> Option<Group> {
        let token = self.tokens.get(index).filter(|token| token.is_word())?;
        if !self.numbers.articles.contains(&token.folded().as_str()) {
            return None;
        }
        let next = self.next_word(index, false)?;
        matches!(self.word(next)?.term, Term::Hundred | Term::Scale(_)).then_some(Group {
            value: 1,
            last: index,
            words: 1,
            ordinal: None,
        })
    }

    /// A digit, teen or tens word, a tens word joined to its digit ("twenty five", "twenty-five").
    fn below_100(&self, index: usize) -> Option<Group> {
        let word = self.word(index)?;
        let single = |value| Group {
            value,
            last: index,
            words: 1,
            ordinal: word.ordinal,
        };
        match word.term {
            Term::Digit(value) | Term::Teen(value) => Some(single(value)),
            Term::Tens(tens) if word.ordinal.is_none() => {
                let unit = self
                    .next_word(index, true)
                    .and_then(|at| Some((at, self.word(at)?)));
                match unit {
                    Some((
                        at,
                        Word {
                            term: Term::Digit(unit @ 1..),
                            ordinal,
                        },
                    )) => Some(Group {
                        value: tens + unit,
                        last: at,
                        words: 2,
                        ordinal,
                    }),
                    _ => Some(single(tens)),
                }
            }
            Term::Tens(tens) => Some(single(tens)),
            Term::Hundred | Term::Scale(_) => None,
        }
    }

    /// `head` times the hundred when the hundred follows it, plus what follows the hundred ("one hundred and five").
    fn hundreds(&self, head: Group) -> Group {
        if head.ordinal.is_some() || head.value == 0 {
            return head;
        }
        let hundred = self
            .next_word(head.last, false)
            .and_then(|at| Some((at, self.word(at)?)));
        let Some((
            at,
            Word {
                term: Term::Hundred,
                ordinal,
            },
        )) = hundred
        else {
            return head;
        };
        let hundreds = Group {
            value: head.value * 100,
            last: at,
            words: head.words + 1,
            ordinal,
        };
        if ordinal.is_some() {
            return hundreds;
        }
        match self.addend(at, |start| self.below_100(start)) {
            Some((tail, joined)) => Group {
                value: hundreds.value + tail.value,
                last: tail.last,
                words: hundreds.words + joined + tail.words,
                ordinal: tail.ordinal,
            },
            None => hundreds,
        }
    }

    /// The group `parse` reads after the hundred or a scale at `at`, past an optional joiner, and the joiner count.
    fn addend(&self, at: usize, parse: impl Fn(usize) -> Option<Group>) -> Option<(Group, usize)> {
        let next = self.next_word(at, false)?;
        let (start, joined) = if self.is(next, self.numbers.joiner) {
            (self.next_word(next, false)?, 1)
        } else {
            (next, 0)
        };
        parse(start)
            .filter(|group| group.value > 0)
            .map(|group| (group, joined))
    }

    /// The scale word right after `last`: its index, value and word.
    fn scale_after(&self, last: usize) -> Option<(usize, u64, Word)> {
        let at = self.next_word(last, false)?;
        let word = self.word(at)?;
        match word.term {
            Term::Scale(scale) => Some((at, scale, word)),
            _ => None,
        }
    }

    /// "point" and the digit words after `last`: the digits, the last index and the words read.
    fn fraction(&self, last: usize) -> Option<(String, usize, usize)> {
        let point = self
            .next_word(last, false)
            .filter(|at| self.is(*at, self.numbers.decimal_point))?;
        let (mut digits, mut last, mut words) = (String::new(), point, 1);
        while let Some((at, digit)) = self
            .next_word(last, false)
            .and_then(|at| Some((at, self.digit(at)?)))
        {
            digits.push(digit);
            last = at;
            words += 1;
        }
        (!digits.is_empty()).then_some((digits, last, words))
    }

    /// `number` with the percent word after it, when there is one.
    fn with_percent(&self, mut number: Number) -> Number {
        if number.ordinal.is_none()
            && number.scale_word.is_none()
            && let Some(at) = self
                .next_word(number.last, false)
                .filter(|at| self.is(*at, self.numbers.percent))
        {
            number.percent = true;
            number.last = at;
            number.words += 1;
        }
        number
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: clock time, spoken year, ten thirty, nineteen eighty four, oh digit, number pair
     * WHAT:  When a one-word number is followed by a second number from 10 to 99 (or "oh" and a digit), the pair
     *        written as a clock time ("ten thirty" → "10:30") or a year ("fifteen twenty three" → "1523"), and the
     *        pair's last index.
     * WHY:   People say times and years as two numbers; adding them ("10 30" → 40) would be wrong and writing both
     *        apart ("15 23") unreadable. A clock time wins when the first is an hour and the second can be minutes;
     *        otherwise a year needs a century from the lexicon, so "fifty fifty" stays two numbers. A pair that goes
     *        on ("nineteen twenty five hundred", "ten thirty percent") is not a time or a year.
     * WHERE: digits_at, before the single-phrase rules.
     */
    fn clock_or_year(&self, first: &Number) -> Option<(String, usize)> {
        if first.article || first.words != 1 || first.ordinal.is_some() {
            return None;
        }
        let start = self.next_word(first.last, false)?;
        let (second, last) = if self.is_zero_digit(start) {
            let at = self.next_word(start, false)?;
            match self.word(at)? {
                Word {
                    term: Term::Digit(digit @ 1..),
                    ordinal: None,
                } => (digit, at),
                _ => return None,
            }
        } else {
            let group = self
                .below_100(start)
                .filter(|group| group.ordinal.is_none() && group.value >= 10)?;
            (group.value, group.last)
        };
        let continues = self.next_word(last, false).is_some_and(|at| {
            self.is(at, self.numbers.decimal_point)
                || self.is(at, self.numbers.percent)
                || self
                    .word(at)
                    .is_some_and(|word| matches!(word.term, Term::Hundred | Term::Scale(_)))
        });
        if continues {
            return None;
        }
        let numbers = self.numbers;
        let text = if numbers.clock_hours.contains(&first.whole) && second <= 59 {
            format!("{}{}{second:02}", first.whole, numbers.time_separator)
        } else if numbers.year_centuries.contains(&first.whole) {
            format!("{}{second:02}", first.whole)
        } else {
            return None;
        };
        Some((text, last))
    }

    /// The phrase is written in digits: it is more than a lone small word, a bare "a hundred" or an ambiguous ordinal.
    fn worth_digits(&self, number: &Number) -> bool {
        let ambiguous = number.ordinal.is_some()
            && self.tokens.get(number.last).is_some_and(|token| {
                self.numbers
                    .ambiguous_ordinals
                    .contains(&token.folded().as_str())
            });
        let bare_article = number.article && number.words == 2 && !number.percent;
        !ambiguous
            && !bare_article
            && (number.words > 1 || number.whole >= self.numbers.digits_from)
    }

    /// `number` in digits.
    fn write(&self, number: &Number) -> String {
        let numbers = self.numbers;
        let mut text = if number.whole >= numbers.group_from {
            grouped(number.whole, numbers.group_separator)
        } else {
            number.whole.to_string()
        };
        if !number.fraction.is_empty() {
            text.push(numbers.decimal_separator);
            text.push_str(&number.fraction);
        }
        if let Some(scale) = number.scale_word.and_then(|at| self.tokens.get(at)) {
            text.push(' ');
            text.push_str(&scale.folded());
        }
        if let Some(suffix) = number.ordinal {
            text.push_str(suffix);
        }
        if number.percent {
            text.push(numbers.percent_sign);
        }
        text
    }

    /// The number word at `index`.
    fn word(&self, index: usize) -> Option<Word> {
        let folded = self
            .tokens
            .get(index)
            .filter(|token| token.is_word())?
            .folded();
        if let Some(value) = self.numbers.cardinal(&folded) {
            return Some(Word {
                term: Term::of(value),
                ordinal: None,
            });
        }
        let (value, suffix) = self.numbers.ordinal(&folded)?;
        Some(Word {
            term: Term::of(value),
            ordinal: Some(suffix),
        })
    }

    /// The digit a word stands for inside decimals: a digit word or a zero word ("oh").
    fn digit(&self, index: usize) -> Option<char> {
        if self.is_zero_digit(index) {
            return Some('0');
        }
        match self.word(index)? {
            Word {
                term: Term::Digit(value),
                ordinal: None,
            } => char::from_digit(u32::try_from(value).ok()?, 10),
            _ => None,
        }
    }

    fn is_zero_digit(&self, index: usize) -> bool {
        self.numbers
            .zero_digits
            .iter()
            .any(|zero| self.is(index, zero))
    }

    /// The token at `index` is the word `word` (compared folded).
    fn is(&self, index: usize, word: &str) -> bool {
        self.tokens
            .get(index)
            .is_some_and(|token| token.is_word() && token.folded() == word)
    }

    /// The index of the word after the word at `index`, when one space (or, with `hyphen`, a hyphen) joins them.
    fn next_word(&self, index: usize, hyphen: bool) -> Option<usize> {
        let gap = self.tokens.get(index + 1)?;
        let joins =
            (gap.is_space() && !gap.is_line_break()) || (hyphen && gap.punct() == Some('-'));
        (joins && self.tokens.get(index + 2).is_some_and(Token::is_word)).then_some(index + 2)
    }
}

/// `value` with `separator` between groups of three digits ("12,345").
fn grouped(value: u64, separator: char) -> String {
    let digits = value.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(separator);
        }
        text.push(digit);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{
        super::{lexicon::LEXICONS, text::render},
        *,
    };

    fn run(text: &str) -> String {
        let english = LEXICONS[0].numbers.expect("English has number words");
        render(&write_numbers(&tokenize(text), english))
    }

    #[test]
    fn writes_spoken_numbers_in_digits() {
        let cases = [
            ("twenty, fifty", "20, 50"),
            ("Twenty, fifty.", "20, 50."),
            ("twenty five", "25"),
            ("Twenty-five people came.", "25 people came."),
            ("ninety nine", "99"),
            ("one thousand five hundred twenty three", "1523"),
            ("one thousand five hundred and twenty three", "1523"),
            ("fifteen hundred twenty three", "1523"),
            ("two thousand twenty six", "2026"),
            ("two thousand and five", "2005"),
            ("one hundred people", "100 people"),
            ("a hundred and ten", "110"),
            ("three hundred thousand", "300,000"),
            ("ten thousand", "10,000"),
            ("twelve thousand three hundred forty five", "12,345"),
            ("two million five hundred thousand", "2,500,000"),
            ("two million", "2 million"),
            ("two million people", "2 million people"),
            ("three point five billion", "3.5 billion"),
            ("three point one four", "3.14"),
            ("zero point five", "0.5"),
            ("two point oh", "2.0"),
            ("fifty percent", "50%"),
            ("five percent", "5%"),
            ("a hundred percent", "100%"),
            ("twenty first century", "21st century"),
            ("twenty-third", "23rd"),
            ("the twentieth", "the 20th"),
            ("one hundredth", "100th"),
            ("the eleventh", "the 11th"),
            ("ten apples", "10 apples"),
            ("ten-year-old", "10-year-old"),
            ("room twenty one, twenty two", "room 21, 22"),
            ("twenty and thirty", "20 and 30"),
            ("Seventy-Five", "75"),
        ];
        for (input, expected) in cases {
            assert_eq!(run(input), expected, "{input:?}");
        }
    }

    #[test]
    fn writes_clock_times_and_years() {
        let cases = [
            ("fifteen twenty three", "1523"),
            ("nineteen eighty four", "1984"),
            ("twenty twenty six", "2026"),
            ("twenty twenty-six", "2026"),
            ("nineteen oh five", "1905"),
            ("twenty oh five", "2005"),
            ("meet at ten thirty", "meet at 10:30"),
            ("at five thirty", "at 5:30"),
            ("ten oh five", "10:05"),
            ("twelve fifteen", "12:15"),
            ("fifty fifty", "50 50"),
        ];
        for (input, expected) in cases {
            assert_eq!(run(input), expected, "{input:?}");
        }
    }

    #[test]
    fn keeps_words_that_are_not_clearly_numbers() {
        let unchanged = [
            "I have five apples",
            "one of them",
            "no one knows",
            "the fifth",
            "one",
            "one two three",
            "a hundred people",
            "a million reasons",
            "a twenty second clip",
            "wait a second",
            "two point",
            "point five",
            "hundred",
            "the first time",
            "room 10 10",
            "",
        ];
        for input in unchanged {
            assert_eq!(run(input), input, "{input:?}");
        }
        assert_eq!(run("twenty seconds"), "20 seconds");
        assert_eq!(
            run("twenty\nfive"),
            "20\nfive",
            "a line break ends a number"
        );
        assert_eq!(run("twenty. Five"), "20. Five");
    }

    #[test]
    fn groups_large_numbers() {
        assert_eq!(grouped(1_523, ','), "1,523");
        assert_eq!(grouped(10_000, ','), "10,000");
        assert_eq!(grouped(999, ','), "999");
        assert_eq!(grouped(1_234_567, ','), "1,234,567");
    }

    #[test]
    fn reads_every_scale() {
        let huge = "nine hundred ninety nine trillion nine hundred ninety nine billion";
        assert_eq!(run(huge), "999,999,000,000,000");
    }
}
