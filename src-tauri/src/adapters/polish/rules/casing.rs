/*!
 * SOURCE OF TRUTH KEYWORDS: casing stage, sentence case, capitalize sentence start, standalone i, abbreviations, sentence end detection
 * WHAT:  `apply_casing` capitalizes the first word of the text, of each line and of each sentence, and the words the
 *        lexicon always capitalizes ("i" → "I", "i'm" → "I'm").
 * WHY:   Stage 5 of 02 §8.3, skipped when the engine's caps say it cases its own output (rules/mod.rs decides). It
 *        only ever raises a letter, never lowers one, so names the engine wrote stay, and it leaves a word that already
 *        holds a capital alone, so a deliberate spelling ("iPhone", a dictionary term such as "eBay") survives a
 *        sentence start. A full stop ends a sentence only
 *        when whitespace or the end follows it and it is not part of an ellipsis, a one-letter abbreviation chain
 *        ("e.g.", "U.S.") or a lexicon abbreviation ("Dr."); "3.5" and "example.com" never qualify because a word
 *        follows the dot directly. An ellipsis does not end a sentence: it usually marks a trailing thought.
 * WHERE: RulePolisher::apply (rules/mod.rs), last rule stage, when `PolishContext.cased` is false.
 */

use super::{
    lexicon::Lexicon,
    text::{Token, TokenKind, capitalize_first},
};

/// Capitalizes sentence starts and the lexicon's capitalized words in place.
pub fn apply_casing(tokens: &mut [Token], lexicon: Option<&Lexicon>) {
    let mut sentence_start = true;
    for index in 0..tokens.len() {
        match tokens[index].kind {
            TokenKind::Word => {
                // "i" in "i.e." is part of an abbreviation, not the pronoun.
                let dotted = tokens.get(index + 1).and_then(Token::punct) == Some('.')
                    && tokens.get(index + 2).is_some_and(Token::is_word);
                let always = !dotted
                    && lexicon
                        .is_some_and(|lexicon| lexicon.is_capitalized(&tokens[index].folded()));
                let spelled = tokens[index].text.chars().any(char::is_uppercase);
                if (sentence_start || always) && !spelled {
                    tokens[index].text = capitalize_first(&tokens[index].text);
                }
                sentence_start = false;
            }
            TokenKind::Space => sentence_start |= tokens[index].is_line_break(),
            TokenKind::Punct => sentence_start |= ends_sentence(tokens, index, lexicon),
        }
    }
}

/// The punctuation at `index` ends a sentence.
fn ends_sentence(tokens: &[Token], index: usize, lexicon: Option<&Lexicon>) -> bool {
    let punct_at = |index: usize| tokens.get(index).and_then(Token::punct);
    let before = index.checked_sub(1);
    let followed_by_break = tokens.get(index + 1).is_none_or(|next| {
        next.is_space()
            || next
                .punct()
                .is_some_and(|c| matches!(c, ')' | ']' | '"' | '\u{201d}' | '\u{2019}'))
    });
    match punct_at(index) {
        Some('!' | '?') => followed_by_break,
        Some('.') => {
            let in_ellipsis =
                punct_at(index + 1) == Some('.') || before.and_then(punct_at) == Some('.');
            let abbreviation = before
                .and_then(|before| tokens.get(before))
                .is_some_and(|word| {
                    word.is_word()
                        && ((word.text.chars().count() == 1
                            && before
                                .and_then(|before| before.checked_sub(1))
                                .and_then(punct_at)
                                == Some('.'))
                            || lexicon
                                .is_some_and(|lexicon| lexicon.is_abbreviation(&word.folded())))
                });
            followed_by_break && !in_ellipsis && !abbreviation
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
        let mut tokens = tokenize(text);
        apply_casing(&mut tokens, lexicon);
        render(&tokens)
    }

    #[test]
    fn capitalizes_sentences_and_the_english_i() {
        let english = Some(&LEXICONS[0]);
        let cases = [
            ("hello. world", "Hello. World"),
            (
                "i think i'm right, i\u{2019}ll see",
                "I think I'm right, I\u{2019}ll see",
            ),
            ("what? no! yes.", "What? No! Yes."),
            ("see e.g. this and i.e. that", "See e.g. this and i.e. that"),
            ("call dr. smith now", "Call dr. smith now"),
            ("it costs 3.5 dollars. ok", "It costs 3.5 dollars. Ok"),
            ("visit example.com today", "Visit example.com today"),
            ("wait... then", "Wait... then"),
            ("line\nnext line", "Line\nNext line"),
            (
                "\u{201c}quoted\u{201d} start",
                "\u{201c}Quoted\u{201d} start",
            ),
            ("(yes.) then", "(Yes.) Then"),
            ("plan B. next", "Plan B. Next"),
            ("the iPhone and NASA", "The iPhone and NASA"),
            ("iPhone first. eBay next", "iPhone first. eBay next"),
            ("émile arrived", "Émile arrived"),
        ];
        for (input, expected) in cases {
            assert_eq!(run(input, english), expected, "{input}");
        }
    }

    #[test]
    fn other_languages_keep_i_lowercase() {
        let italian = LEXICONS.iter().find(|lexicon| lexicon.language == "it");
        assert_eq!(run("vedo i ragazzi", italian), "Vedo i ragazzi");
        assert_eq!(run("so i think", None), "So i think");
    }
}
