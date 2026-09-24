/*!
 * SOURCE OF TRUTH KEYWORDS: rule polisher tokens, Token, TokenKind, tokenize, render, word token, punctuation token, capitalize first letter
 * WHAT:  The token model the rule stages share: `tokenize` splits text into words, whitespace runs and single
 *        punctuation characters; `render` joins them back; small helpers classify punctuation and change a word's
 *        first letter.
 * WHY:   Fillers, repeats, spacing and casing all reason about "the word before" and "the punctuation after"; one
 *        tokenizer keeps their notion of a word identical. A word is a run of alphanumeric characters with inner
 *        apostrophes (`don't`, `I'm`, `it’s`), so contractions stay one word. Every punctuation character is its own
 *        token, so `...` is three `.` tokens and stages can tell an ellipsis from a full stop. `tokenize` followed by
 *        `render` returns the input unchanged, so a stage that changes nothing changes nothing.
 * WHERE: rules/mod.rs tokenizes once after the dictionary stage; fillers.rs, repeats.rs, spacing.rs, casing.rs and
 *        lexicon.rs read and rewrite the tokens.
 */

/// What a token holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Word,
    Space,
    Punct,
}

/// A piece of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
}

impl Token {
    pub fn new(kind: TokenKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }

    pub fn is_word(&self) -> bool {
        self.kind == TokenKind::Word
    }

    pub fn is_space(&self) -> bool {
        self.kind == TokenKind::Space
    }

    /// The single punctuation character, when this is a punctuation token.
    pub fn punct(&self) -> Option<char> {
        match self.kind {
            TokenKind::Punct => self.text.chars().next(),
            _ => None,
        }
    }

    /// A whitespace run that breaks the line.
    pub fn is_line_break(&self) -> bool {
        self.is_space() && self.text.contains('\n')
    }

    /// The word folded for comparison.
    pub fn folded(&self) -> String {
        self.text.to_lowercase()
    }

    /// The word starts with an uppercase letter.
    pub fn starts_uppercase(&self) -> bool {
        self.text.chars().next().is_some_and(char::is_uppercase)
    }

    /// The word is made of letters (and inner apostrophes) only: no digits.
    pub fn is_alphabetic(&self) -> bool {
        self.is_word()
            && self
                .text
                .chars()
                .all(|c| c.is_alphabetic() || is_apostrophe(c))
    }
}

/// Apostrophes that join a contraction into one word.
pub const fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}')
}

/// Characters that end a sentence.
pub const fn is_terminal(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '\u{2026}')
}

/// Characters that mark a pause inside a sentence.
pub const fn is_pause(c: char) -> bool {
    matches!(c, ',' | ';' | ':')
}

/// Dashes a speaker's break is written with.
pub const fn is_dash(c: char) -> bool {
    matches!(c, '-' | '\u{2013}' | '\u{2014}')
}

/// Splits `text` into tokens; `render(&tokenize(text)) == text`.
pub fn tokenize(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while let Some(&c) = chars.get(index) {
        let start = index;
        let kind = if c.is_whitespace() {
            while chars.get(index).is_some_and(|c| c.is_whitespace()) {
                index += 1;
            }
            TokenKind::Space
        } else if c.is_alphanumeric() {
            while let Some(&next) = chars.get(index) {
                let inner_apostrophe = is_apostrophe(next)
                    && chars
                        .get(index + 1)
                        .is_some_and(|after| after.is_alphanumeric());
                if !next.is_alphanumeric() && !inner_apostrophe {
                    break;
                }
                index += 1;
            }
            TokenKind::Word
        } else {
            index += 1;
            TokenKind::Punct
        };
        tokens.push(Token::new(
            kind,
            chars[start..index].iter().collect::<String>(),
        ));
    }
    tokens
}

/// Joins tokens back into text.
pub fn render(tokens: &[Token]) -> String {
    tokens.iter().map(|token| token.text.as_str()).collect()
}

/// `word` with its first character uppercased (`i'm` → `I'm`, `émile` → `Émile`).
pub fn capitalize_first(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The index of the closest token before `index` that is not whitespace.
pub fn previous_significant(tokens: &[Token], index: usize) -> Option<usize> {
    tokens[..index.min(tokens.len())]
        .iter()
        .rposition(|token| !token.is_space())
}

/// The index of the closest token at or after `index` that is not whitespace.
pub fn next_significant(tokens: &[Token], index: usize) -> Option<usize> {
    tokens
        .get(index..)?
        .iter()
        .position(|token| !token.is_space())
        .map(|offset| index + offset)
}
