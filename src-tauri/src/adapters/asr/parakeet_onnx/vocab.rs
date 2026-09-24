/*!
 * SOURCE OF TRUTH KEYWORDS: Parakeet vocabulary, vocab.txt, SentencePiece tokens, blank token, <blk>, detokenize, token spacing, special tokens
 * WHAT:  Vocabulary: the Parakeet token table parsed from `vocab.txt` (`<token> <id>` per line, `▁` marking a word
 *        start), the blank id the TDT decoder needs, and `decode` which turns emitted token ids into text.
 * WHY:   The ONNX export ships its SentencePiece table as plain text (05 A1), so no tokenizer library is needed.
 *        Ids must cover 0..n exactly once and `<blk>` must exist, otherwise the decoder would index past the table
 *        or never stop on blank; a table that breaks either rule is a corrupt model, not a crash. Spacing follows
 *        the reference implementation (onnx-asr): `▁` becomes a space, a space is kept only before a word character,
 *        so "▁hello ▁," joins to "hello,". Control tokens (`<unk>`, `<|endoftext|>`, `<|en|>`…) are never text.
 * WHERE: Loaded by ParakeetOnnx::load (parakeet_onnx/mod.rs); `decode` runs after every TDT decode.
 */

/// The word-start marker SentencePiece puts in front of a token.
const WORD_START: char = '\u{2581}';

/// The blank token the TDT decoder emits when a frame holds no new token.
const BLANK: &str = "<blk>";

struct Token {
    text: String,
    /// Control tokens (`<unk>`, `<|…|>`) and the blank never become text.
    silent: bool,
}

/// The Parakeet token table.
pub struct Vocabulary {
    tokens: Vec<Token>,
    blank: usize,
}

impl Vocabulary {
    /// Parses `vocab.txt`; the error describes the first problem (it becomes `ModelCorrupt` detail).
    pub fn parse(content: &str) -> Result<Self, String> {
        let mut slots: Vec<Option<Token>> = Vec::new();
        let mut blank = None;
        for (number, line) in content.lines().enumerate() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let (text, id) = line
                .rsplit_once(' ')
                .ok_or_else(|| format!("vocab.txt line {}: expected `<token> <id>`", number + 1))?;
            let id: usize = id
                .parse()
                .map_err(|_| format!("vocab.txt line {}: `{id}` is not a token id", number + 1))?;
            if slots.len() <= id {
                slots.resize_with(id + 1, || None);
            }
            if slots[id].is_some() {
                return Err(format!("vocab.txt lists token id {id} twice"));
            }
            if text == BLANK {
                blank = Some(id);
            }
            slots[id] = Some(Token {
                silent: text == BLANK || is_control(text),
                text: text.replace(WORD_START, " "),
            });
        }
        let tokens = slots
            .into_iter()
            .enumerate()
            .map(|(id, token)| token.ok_or_else(|| format!("vocab.txt has no token id {id}")))
            .collect::<Result<Vec<_>, _>>()?;
        let blank = blank.ok_or_else(|| format!("vocab.txt has no {BLANK} token"))?;
        Ok(Self { tokens, blank })
    }

    /// Number of tokens, blank included: the size of the decoder's token logits.
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn blank(&self) -> usize {
        self.blank
    }

    /// Joins emitted token ids into trimmed text with reference spacing; unknown ids and control tokens are skipped.
    pub fn decode(&self, ids: &[usize]) -> String {
        let joined: String = ids
            .iter()
            .filter_map(|id| self.tokens.get(*id))
            .filter(|token| !token.silent)
            .map(|token| token.text.as_str())
            .collect();
        let mut text = String::with_capacity(joined.len());
        let mut chars = joined.chars().peekable();
        while let Some(character) = chars.next() {
            if !character.is_whitespace() {
                text.push(character);
            } else if !text.is_empty() && chars.peek().is_some_and(|next| is_word_char(*next)) {
                // A space survives only in front of a word character, so none precede punctuation or pile up.
                text.push(' ');
            }
        }
        text
    }
}

/// `<unk>`, `<pad>`, `<|endoftext|>`, `<|en|>`: SentencePiece control tokens.
fn is_control(text: &str) -> bool {
    text.len() > 2 && text.starts_with('<') && text.ends_with('>')
}

/// A regex `\w` character: letters and digits of any script, and `_`.
fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "<unk> 0\n<|endoftext|> 1\n▁hello 2\n▁world 3\n, 4\n. 5\n▁ 6\n' 7\ns 8\n▁Grüße 9\n<blk> 10\n";

    fn ids(vocab: &Vocabulary, pieces: &[&str]) -> Vec<usize> {
        pieces
            .iter()
            .map(|piece| {
                let text = piece.replace(WORD_START, " ");
                vocab
                    .tokens
                    .iter()
                    .position(|token| token.text == text)
                    .unwrap()
            })
            .collect()
    }

    #[test]
    fn parses_ids_blank_and_word_markers() {
        let vocab = Vocabulary::parse(SAMPLE).unwrap();
        assert_eq!(vocab.len(), 11);
        assert_eq!(vocab.blank(), 10);
        assert_eq!(vocab.tokens[2].text, " hello");
        assert!(vocab.tokens[0].silent && vocab.tokens[1].silent && vocab.tokens[10].silent);
        assert!(!vocab.tokens[4].silent);
    }

    #[test]
    fn decode_spaces_words_and_attaches_punctuation() {
        let vocab = Vocabulary::parse(SAMPLE).unwrap();
        let text = vocab.decode(&ids(&vocab, &["▁hello", ",", "▁world", "'", "s", "▁", "."]));
        assert_eq!(text, "hello, world's.");
        assert_eq!(vocab.decode(&ids(&vocab, &["▁Grüße", "."])), "Grüße.");
        assert_eq!(
            vocab.decode(&[0, 1, 10, 2, 99]),
            "hello",
            "control, blank and unknown ids are silent"
        );
        assert_eq!(vocab.decode(&[]), "");
    }

    #[test]
    fn broken_tables_are_rejected() {
        assert!(
            Vocabulary::parse("▁a 0\n▁b 2\n<blk> 3\n").is_err(),
            "a gap in the ids"
        );
        assert!(
            Vocabulary::parse("▁a 0\n▁b 0\n<blk> 1\n").is_err(),
            "a duplicate id"
        );
        assert!(Vocabulary::parse("▁a 0\n▁b 1\n").is_err(), "no blank");
        assert!(Vocabulary::parse("▁a zero\n<blk> 1\n").is_err(), "a bad id");
        assert!(
            Vocabulary::parse("nospace\n<blk> 1\n").is_err(),
            "a line without an id"
        );
        assert!(
            Vocabulary::parse("▁a 0\r\n<blk> 1\r\n").is_ok(),
            "CRLF line endings"
        );
    }
}
