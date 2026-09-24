/*!
 * SOURCE OF TRUTH KEYWORDS: filler removal, remove fillers, um uh er, you know at clause edges, edge fillers, capital transfer
 * WHAT:  `remove_fillers` drops the lexicon's hesitation words wherever they stand and its edge phrases ("you know")
 *        only at a clause edge, together with the pause mark written right after them ("um," "uh…" "er—").
 * WHY:   Stage 2 of 02 §8.3, behind `polish.remove_fillers`. A phrase like "you know" is only filler when it stands
 *        alone between clause boundaries ("You know, it works" / "it works, you know."); "do you know him" keeps it.
 *        Punctuation before the filler is left for the spacing stage, which folds ", ." into "." and drops a
 *        leading comma, so each stage stays small. A filler that opened a sentence with a capital hands the capital
 *        to the next word ("Um, so" → "So"), because casing is skipped for engines that case their own output.
 * WHERE: RulePolisher::apply (rules/mod.rs), after the dictionary and before repeats, when the context asks for it
 *        and a lexicon was resolved.
 */

use super::{
    lexicon::Lexicon,
    text::{
        Token, capitalize_first, is_dash, is_pause, is_terminal, next_significant,
        previous_significant,
    },
};

/// `tokens` without the lexicon's fillers.
pub fn remove_fillers(tokens: &[Token], lexicon: &Lexicon) -> Vec<Token> {
    let mut dropped = vec![false; tokens.len()];
    let mut index = 0;
    while index < tokens.len() {
        let Some(end) = filler_end(tokens, index, lexicon) else {
            index += 1;
            continue;
        };
        let start = index;
        dropped[start..end].fill(true);
        index = end;
        let mut closing_comma = false;
        // The pause mark written against the filler belongs to it ("um," "um..." "um—").
        while let Some(token) = tokens.get(index) {
            let belongs = token.punct().is_some_and(|c| {
                c == ','
                    || c == '\u{2026}'
                    || is_dash(c)
                    || (c == '.' && is_ellipsis_dot(tokens, index))
            });
            if !belongs {
                break;
            }
            closing_comma |= token.punct() == Some(',');
            dropped[index] = true;
            index += 1;
        }
        // A filler set off by a pair of commas takes both with it ("I think, um, we" → "I think we").
        if closing_comma
            && let Some(before) = previous_significant(tokens, start)
            && tokens[before].punct() == Some(',')
        {
            dropped[before] = true;
        }
    }
    emit(tokens, &dropped)
}

/// The end (exclusive) of the filler starting at `index`, if one does.
fn filler_end(tokens: &[Token], index: usize, lexicon: &Lexicon) -> Option<usize> {
    let token = tokens.get(index).filter(|token| token.is_word())?;
    if lexicon.is_filler(&token.folded()) {
        return Some(index + 1);
    }
    lexicon
        .edge_fillers
        .iter()
        .find_map(|phrase| phrase_end(tokens, index, phrase))
        .filter(|end| at_clause_edge(tokens, index, *end))
}

/// The end of `phrase` when its words start at `index`, separated by single-line whitespace.
fn phrase_end(tokens: &[Token], index: usize, phrase: &[&str]) -> Option<usize> {
    let mut cursor = index;
    for (position, word) in phrase.iter().enumerate() {
        if position > 0 {
            tokens
                .get(cursor)
                .filter(|token| token.is_space() && !token.is_line_break())?;
            cursor += 1;
        }
        let token = tokens.get(cursor).filter(|token| token.is_word())?;
        if token.folded() != *word {
            return None;
        }
        cursor += 1;
    }
    Some(cursor)
}

/// Tokens `start..end` stand between clause boundaries.
fn at_clause_edge(tokens: &[Token], start: usize, end: usize) -> bool {
    let is_boundary = |index: Option<usize>| {
        index
            .and_then(|index| tokens.get(index))
            .is_none_or(|token| {
                token
                    .punct()
                    .is_some_and(|c| is_pause(c) || is_terminal(c) || is_dash(c))
            })
    };
    is_boundary(previous_significant(tokens, start)) && is_boundary(next_significant(tokens, end))
}

/// The `.` at `index` is part of a run of two or more dots.
fn is_ellipsis_dot(tokens: &[Token], index: usize) -> bool {
    let is_dot = |index: usize| tokens.get(index).and_then(Token::punct) == Some('.');
    is_dot(index) && (is_dot(index + 1) || index.checked_sub(1).is_some_and(is_dot))
}

/// The kept tokens; a dropped filler that opened a sentence with a capital capitalizes the next kept word.
fn emit(tokens: &[Token], dropped: &[bool]) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut pending_capital = false;
    for (token, dropped) in tokens.iter().zip(dropped) {
        if *dropped {
            if token.is_word() && token.starts_uppercase() && opens_sentence(&out) {
                pending_capital = true;
            }
            continue;
        }
        if token.punct().is_some_and(is_terminal) {
            pending_capital = false;
        }
        if token.is_word() && std::mem::take(&mut pending_capital) {
            out.push(Token::new(token.kind, capitalize_first(&token.text)));
            continue;
        }
        out.push(token.clone());
    }
    out
}

/// Nothing but whitespace, pause marks or a sentence end precedes this point.
fn opens_sentence(out: &[Token]) -> bool {
    out.iter()
        .rev()
        .find(|token| !token.is_space() && !token.punct().is_some_and(is_pause))
        .is_none_or(|token| token.punct().is_some_and(is_terminal) || token.is_line_break())
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

    fn run(text: &str) -> String {
        render(&remove_fillers(&tokenize(text), &LEXICONS[0]))
    }

    #[test]
    fn removes_fillers_and_the_pause_mark_after_them() {
        let cases = [
            ("Um, so we start.", " So we start."),
            ("um, so we start.", " so we start."),
            ("We should go, um.", "We should go, ."),
            ("I think, uh, we should", "I think  we should"),
            ("Well um... okay", "Well  okay"),
            ("Er\u{2014} fine", " Fine"),
            ("hmm", ""),
            ("Umbrella and hummus", "Umbrella and hummus"),
            ("The ERM plan", "The  plan"),
            ("Uh, um, yes", "  Yes"),
            ("Hello. Um, that works.", "Hello.  That works."),
        ];
        for (input, expected) in cases {
            assert_eq!(run(input), expected, "{input}");
        }
    }

    #[test]
    fn edge_phrases_go_only_at_clause_edges() {
        let cases = [
            ("You know, it works.", " It works."),
            ("It works, you know.", "It works, ."),
            ("It works, you know", "It works, "),
            ("Do you know him?", "Do you know him?"),
            ("you know what I mean", "you know what I mean"),
            ("It is, you know, fine", "It is  fine"),
        ];
        for (input, expected) in cases {
            assert_eq!(run(input), expected, "{input}");
        }
    }
}
