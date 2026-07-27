//! A deliberately tiny, hand-written tokenizer for `fslite-command`'s line
//! grammar. It is not a shell: there is no expansion of any kind (globs,
//! `$VAR`, `~`, command substitution) and no shell metacharacter (`|`, `;`,
//! `&`, `<`, `>`, backtick, `$(`) is ever treated as literal text when it
//! appears unquoted — it is rejected outright, so a user who pastes a real
//! shell command gets a clear error instead of a confusing partial parse.

/// The maximum accepted input line length, checked before any allocation
/// proportional to the input beyond the raw string itself.
pub const MAX_LINE_LEN: usize = 65536;

const REJECTED_UNQUOTED_METACHARACTERS: &[char] = &['|', ';', '&', '<', '>', '`'];

/// One lexical token: a bare word/path, or a `--flag[=value]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Token {
    /// A positional argument (verb or path).
    Word(String),
    /// A `--name` or `--name=value` flag.
    Flag { name: String, value: Option<String> },
}

/// Why a line could not be tokenized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LexError {
    /// A `'` or `"` was opened but never closed.
    UnterminatedQuote,
    /// A `\` inside a double-quoted string preceded an unsupported character.
    InvalidEscape(char),
    /// The input contained a NUL byte.
    NulByte,
    /// The input exceeded [`MAX_LINE_LEN`], checked before tokenizing.
    TooLong { max: usize, actual: usize },
    /// An unquoted shell metacharacter appeared outside a quoted token.
    UnsupportedMetacharacter(char),
}

/// Tokenizes one line of `fslite-command` grammar.
pub fn tokenize(line: &str) -> Result<Vec<Token>, LexError> {
    if line.len() > MAX_LINE_LEN {
        return Err(LexError::TooLong { max: MAX_LINE_LEN, actual: line.len() });
    }
    if line.contains('\0') {
        return Err(LexError::NulByte);
    }
    // `$(` is checked as a two-character sequence; single '$' and '~' are
    // never rejected or expanded — see the `dollar_and_tilde_are_never_expanded` test.
    if line.contains("$(") {
        return Err(LexError::UnsupportedMetacharacter('$'));
    }

    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if REJECTED_UNQUOTED_METACHARACTERS.contains(&ch) {
            return Err(LexError::UnsupportedMetacharacter(ch));
        }

        let word = read_word(&mut chars)?;
        tokens.push(classify(word));
    }

    Ok(tokens)
}

fn classify(word: String) -> Token {
    match word.strip_prefix("--") {
        Some(rest) => match rest.split_once('=') {
            Some((name, value)) => Token::Flag { name: name.to_string(), value: Some(value.to_string()) },
            None => Token::Flag { name: rest.to_string(), value: None },
        },
        None => Token::Word(word),
    }
}

fn read_word(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, LexError> {
    let mut word = String::new();
    // Whether this word has already consumed *any* input, including a
    // quoted segment that happened to yield zero characters (e.g. `''` or
    // `""`). `!word.is_empty()` is NOT an equivalent proxy for this: an
    // empty quoted segment leaves `word` empty even though a token has
    // definitely started, which would let a metacharacter immediately
    // following it (e.g. `'';rm -rf /`) fall through as literal text
    // instead of being rejected.
    let mut started = false;

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            break;
        }
        if REJECTED_UNQUOTED_METACHARACTERS.contains(&ch) && started {
            // A metacharacter ending a word (e.g. `foo;`) is still rejected —
            // stop and let the outer loop's boundary check on the *next*
            // iteration catch it. To fail immediately rather than silently
            // absorbing it as a separate empty word, check right here too.
            return Err(LexError::UnsupportedMetacharacter(ch));
        }

        match ch {
            '\'' => {
                chars.next();
                word.push_str(&read_single_quoted(chars)?);
                started = true;
            }
            '"' => {
                chars.next();
                word.push_str(&read_double_quoted(chars)?);
                started = true;
            }
            _ => {
                word.push(ch);
                chars.next();
                started = true;
            }
        }
    }

    Ok(word)
}

fn read_single_quoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, LexError> {
    let mut content = String::new();
    loop {
        match chars.next() {
            None => return Err(LexError::UnterminatedQuote),
            Some('\'') => return Ok(content),
            Some(ch) => content.push(ch),
        }
    }
}

fn read_double_quoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, LexError> {
    let mut content = String::new();
    loop {
        match chars.next() {
            None => return Err(LexError::UnterminatedQuote),
            Some('"') => return Ok(content),
            Some('\\') => match chars.next() {
                None => return Err(LexError::UnterminatedQuote),
                Some('n') => content.push('\n'),
                Some('t') => content.push('\t'),
                Some('"') => content.push('"'),
                Some('\\') => content.push('\\'),
                Some(other) => return Err(LexError::InvalidEscape(other)),
            },
            Some(ch) => content.push(ch),
        }
    }
}
