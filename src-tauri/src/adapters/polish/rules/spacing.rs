/*!
 * SOURCE OF TRUTH KEYWORDS: spacing stage, punctuation normalization, space before punctuation, double spaces, segment join spacing, stray comma, leading punctuation
 * WHAT:  `normalize_spacing` rewrites whitespace and punctuation: one space between words (line breaks kept), no
 *        space before closing punctuation or after opening brackets, a space after a comma glued to the next word,
 *        stacked marks folded (", ." → ".", ",," → ",", "?." → "?", ". ." → "."), and no punctuation or whitespace
 *        left dangling at the start or a comma at the end.
 * WHY:   Stage 4 of 02 §8.3 and the spacing half of 05 A3: segments joined by the pipeline and words removed by the
 *        filler and repeat stages leave exactly these artefacts ("go, um." → "go, ." → "go."). It is one pass that
 *        looks back at what it has already written, so cascades ("a , , .") settle without repeating the pass.
 *        Dots glued together ("...") are an ellipsis and stay; a full stop is never glued to the next word, so
 *        "3.5", "example.com" and "e.g." are untouched; no space is added after a colon ("10:30").
 * WHERE: RulePolisher::apply (rules/mod.rs), after repeats and before casing; it always runs.
 */

use super::text::{Token, TokenKind, is_pause, is_terminal};

/// Marks that attach to the word before them.
const fn is_closing(c: char) -> bool {
    is_pause(c)
        || is_terminal(c)
        || matches!(
            c,
            ')' | ']' | '}' | '\u{201d}' | '\u{2019}' | '\u{00bb}' | '%'
        )
}

/// Marks that attach to the word after them.
const fn is_opening(c: char) -> bool {
    matches!(
        c,
        '(' | '[' | '{' | '\u{201c}' | '\u{2018}' | '\u{00ab}' | '\u{00bf}' | '\u{00a1}'
    )
}

/// Marks that cannot start a text.
const fn is_dropped_at_start(c: char) -> bool {
    is_pause(c) || matches!(c, '.' | '!' | '?')
}

/// Marks after which a word glued to them gets a space.
const fn wants_space_after(c: char) -> bool {
    matches!(c, ',' | ';' | '!' | '?')
}

/// `tokens` with normalized whitespace and punctuation.
pub fn normalize_spacing(tokens: &[Token]) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::Space => push_space(&mut out, token),
            TokenKind::Punct => push_punct(&mut out, tokens, index),
            TokenKind::Word => push_word(&mut out, token),
        }
    }
    trim_end(&mut out);
    out
}

fn push_space(out: &mut Vec<Token>, token: &Token) {
    let text: String = if token.is_line_break() {
        token
            .text
            .chars()
            .filter(|c| matches!(c, '\r' | '\n'))
            .collect()
    } else {
        " ".to_owned()
    };
    match out.last_mut() {
        None => {}
        Some(last) if last.is_space() => {
            if last.is_line_break() && token.is_line_break() {
                last.text.push_str(&text);
            } else if token.is_line_break() {
                last.text = text;
            }
        }
        Some(last) if last.punct().is_some_and(is_opening) && !token.is_line_break() => {}
        Some(_) => out.push(Token::new(TokenKind::Space, text)),
    }
}

fn push_punct(out: &mut Vec<Token>, tokens: &[Token], index: usize) {
    let token = &tokens[index];
    let Some(c) = token.punct() else { return };
    if out.is_empty() && is_dropped_at_start(c) {
        return;
    }
    let mut spaced = false;
    if is_closing(c) {
        while out
            .last()
            .is_some_and(|last| last.is_space() && !last.is_line_break())
        {
            out.pop();
            spaced = true;
        }
    }
    if is_pause(c) || is_terminal(c) {
        match out.last().and_then(Token::punct) {
            // A pause mark followed by another mark gives way to it (", ." → ".", ",," → ",").
            Some(last) if is_pause(last) => {
                out.pop();
            }
            Some(last) if is_terminal(last) => {
                let next_is_dot = tokens.get(index + 1).and_then(Token::punct) == Some('.');
                let redundant = is_pause(c)
                    || (c == '.' && matches!(last, '!' | '?') && !next_is_dot)
                    || (c == '.' && last == '.' && spaced)
                    || (c == '.' && last == '\u{2026}');
                if redundant {
                    return;
                }
            }
            _ => {}
        }
        if out.is_empty() && is_dropped_at_start(c) {
            return;
        }
    }
    out.push(token.clone());
}

fn push_word(out: &mut Vec<Token>, token: &Token) {
    let glued_after_mark = match out.as_slice() {
        [.., before, mark] => {
            mark.punct().is_some_and(wants_space_after)
                && before.is_word()
                && before.text.chars().last().is_some_and(char::is_alphabetic)
                && token.text.chars().next().is_some_and(char::is_alphabetic)
        }
        _ => false,
    };
    if glued_after_mark {
        out.push(Token::new(TokenKind::Space, " "));
    }
    out.push(token.clone());
}

/// Drops trailing whitespace and pause marks ("It works, " → "It works").
fn trim_end(out: &mut Vec<Token>) {
    while out
        .last()
        .is_some_and(|last| last.is_space() || last.punct().is_some_and(is_pause))
    {
        out.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::text::{render, tokenize},
        *,
    };

    #[test]
    fn normalizes_spacing_and_punctuation() {
        let cases = [
            ("hello . world", "hello. world"),
            ("hello ,world", "hello, world"),
            ("a,,b", "a, b"),
            ("  hello   world  ", "hello world"),
            ("hello . .", "hello."),
            (", so we start", "so we start"),
            (". So we start", "So we start"),
            ("We should go, .", "We should go."),
            ("Really? .", "Really?"),
            ("Really?.", "Really?"),
            ("Hello , , world", "Hello, world"),
            ("I think,  we should", "I think, we should"),
            ("It works, ", "It works"),
            ("wait...", "wait..."),
            ("wait ...", "wait..."),
            ("well, ...", "well..."),
            ("( hello )", "(hello)"),
            ("\u{201c} quoted \u{201d}", "\u{201c}quoted\u{201d}"),
            (
                "3.5 and example.com at 10:30, e.g. this",
                "3.5 and example.com at 10:30, e.g. this",
            ),
            ("50 %", "50%"),
            ("line one.\n\nline two", "line one.\n\nline two"),
            ("line one. \r\n line two", "line one.\r\nline two"),
            ("hello. world", "hello. world"),
            ("Stop!Now", "Stop! Now"),
            ("", ""),
            (" , . ", ""),
        ];
        for (input, expected) in cases {
            assert_eq!(
                render(&normalize_spacing(&tokenize(input))),
                expected,
                "{input:?}"
            );
        }
    }
}
