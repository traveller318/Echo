/*!
 * SOURCE OF TRUTH KEYWORDS: repeats stage, collapse repeated words, stutter removal, the the, hyphen stutter, keep repeats, comma repeats
 * WHAT:  `collapse_repeats` removes stutters ("th- the", "b-but", "I- I think") in any language, then, when the
 *        lexicon allows it, doubled words ("the the" → "the", "I, I think" → "I think").
 * WHY:   Stage 3 of 02 §8.3. A stutter is a fragment the next word starts with, written with a dash; that shape is
 *        orthographic, so it is safe in every language. Joined forms need care: "re-read", "so-so" and "no-no" are
 *        words, so a joined fragment only goes when it is one letter ("b-but") or repeated ("th-th-the"). Doubled
 *        words are language-specific ("had had", "that that" are grammatical English; German "die die" and French
 *        "nous nous" are too), so they collapse only for a lexicon that opts in, never for words on its keep list,
 *        and across a comma only for its short function words. Numbers are never touched ("10 10" may be data),
 *        nor are the lexicon's number words ("twenty twenty" is a year for the numbers stage, which runs next).
 *        The first spelling is kept, and a fragment's capital moves to the word that stays ("B-but" → "But").
 * WHERE: RulePolisher::apply (rules/mod.rs), after fillers and before numbers.
 */

use super::{
    lexicon::Lexicon,
    text::{Token, capitalize_first, is_dash},
};

/// `tokens` without stutters and (per `lexicon`) doubled words.
pub fn collapse_repeats(tokens: &[Token], lexicon: Option<&Lexicon>) -> Vec<Token> {
    let tokens = collapse_stutters(tokens);
    match lexicon.filter(|lexicon| lexicon.collapse_repeats) {
        Some(lexicon) => collapse_doubled_words(&tokens, lexicon),
        None => tokens,
    }
}

fn collapse_stutters(tokens: &[Token]) -> Vec<Token> {
    let mut tokens = tokens.to_vec();
    let mut out = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        if let Some(skip) =
            joined_stutter(&tokens, index).or_else(|| spaced_stutter(&tokens, index))
        {
            // `skip` is the index of the word that stays; it inherits the fragment's capital.
            if tokens[index].starts_uppercase() {
                tokens[skip].text = capitalize_first(&tokens[skip].text);
            }
            index = skip;
            continue;
        }
        out.push(tokens[index].clone());
        index += 1;
    }
    out
}

/// "b-but", "th-th-the", "t-t-test-driven": the index of the first kept word, when `index` starts a stutter.
fn joined_stutter(tokens: &[Token], index: usize) -> Option<usize> {
    let fragment = tokens
        .get(index)
        .filter(|token| token.is_alphabetic())?
        .folded();
    // Words of the hyphen chain starting at `index`, as token indices.
    let mut chain = vec![index];
    let mut cursor = index;
    while tokens
        .get(cursor + 1)
        .and_then(Token::punct)
        .is_some_and(|c| c == '-')
        && tokens.get(cursor + 2).is_some_and(Token::is_alphabetic)
    {
        cursor += 2;
        chain.push(cursor);
    }
    let repeats = chain
        .iter()
        .take_while(|word| tokens[**word].folded() == fragment)
        .count()
        .min(chain.len() - 1);
    let kept = *chain.get(repeats)?;
    let is_stutter = repeats >= 1
        && tokens[kept].folded().starts_with(&fragment)
        && (fragment.chars().count() == 1 || repeats >= 2);
    is_stutter.then_some(kept)
}

/// "th- the", "I- I", "some— something": the index of the word that stays, when `index` starts a stutter.
fn spaced_stutter(tokens: &[Token], index: usize) -> Option<usize> {
    let fragment = tokens
        .get(index)
        .filter(|token| token.is_alphabetic())?
        .folded();
    tokens.get(index + 1)?.punct().filter(|c| is_dash(*c))?;
    tokens
        .get(index + 2)
        .filter(|token| token.is_space() && !token.is_line_break())?;
    let word = tokens
        .get(index + 3)
        .filter(|token| token.is_alphabetic())?;
    word.folded().starts_with(&fragment).then_some(index + 3)
}

fn collapse_doubled_words(tokens: &[Token], lexicon: &Lexicon) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    for token in tokens {
        if token.is_alphabetic() && repeats_previous(&out, token, lexicon) {
            // Drop the gap (and comma) written between the two words, and the second word.
            while out.last().is_some_and(|last| !last.is_word()) {
                out.pop();
            }
            continue;
        }
        out.push(token.clone());
    }
    out
}

/// `word` repeats the word before it: "the the", or "I, I" for the lexicon's comma repeats.
fn repeats_previous(out: &[Token], word: &Token, lexicon: &Lexicon) -> bool {
    let folded = word.folded();
    if lexicon.keeps_repeat(&folded) {
        return false;
    }
    let tail: Vec<&Token> = out.iter().rev().take(3).collect();
    match tail.as_slice() {
        [gap, previous, ..] if gap.is_space() && !gap.is_line_break() && previous.is_word() => {
            previous.folded() == folded
        }
        [gap, comma, previous]
            if gap.is_space()
                && !gap.is_line_break()
                && comma.punct() == Some(',')
                && previous.is_word() =>
        {
            previous.folded() == folded && lexicon.collapses_across_comma(&folded)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{
            lexicon::LEXICONS,
            text::{render, tokenize},
        },
        *,
    };

    fn run(text: &str, lexicon: Option<&Lexicon>) -> String {
        render(&collapse_repeats(&tokenize(text), lexicon))
    }

    #[test]
    fn collapses_doubled_words_for_english() {
        let english = Some(&LEXICONS[0]);
        let cases = [
            ("the the cat", "the cat"),
            ("The the cat", "The cat"),
            ("I I I think", "I think"),
            ("I, I think", "I think"),
            ("the, the problem", "the problem"),
            ("go, go, go", "go, go, go"),
            ("She had had enough", "She had had enough"),
            ("I know that that works", "I know that that works"),
            ("no no no", "no no no"),
            ("room 10 10", "room 10 10"),
            ("stop.\nStop", "stop.\nStop"),
            ("cats\ncats", "cats\ncats"),
        ];
        for (input, expected) in cases {
            assert_eq!(run(input, english), expected, "{input}");
        }
    }

    #[test]
    fn removes_stutters_in_every_language() {
        let cases = [
            ("th- the cat", "the cat"),
            ("I- I think", "I think"),
            ("I- I- I think", "I think"),
            ("b-but why", "but why"),
            ("B-but why", "But why"),
            ("th-th-the end", "the end"),
            ("t-t-test-driven", "test-driven"),
            ("I-I know", "I know"),
            ("some\u{2014} something", "something"),
            (
                "re-read so-so no-no x-ray T-shirt",
                "re-read so-so no-no x-ray T-shirt",
            ),
            ("pre- and post-war", "pre- and post-war"),
            ("well-known", "well-known"),
        ];
        for (input, expected) in cases {
            assert_eq!(run(input, None), expected, "{input}");
        }
    }

    #[test]
    fn languages_without_repeat_rules_keep_doubled_words() {
        let german = LEXICONS.iter().find(|lexicon| lexicon.language == "de");
        assert_eq!(
            run("die Frau, die die Zeitung liest", german),
            "die Frau, die die Zeitung liest"
        );
        assert_eq!(run("the the", None), "the the");
    }
}
