use fslite_command::lexer::{LexError, Token, tokenize};

#[test]
fn splits_on_unquoted_whitespace() {
    let tokens = tokenize("mkdir /docs --parents").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Word("mkdir".into()),
            Token::Word("/docs".into()),
            Token::Flag {
                name: "parents".into(),
                value: None
            },
        ]
    );
}

#[test]
fn single_quotes_are_fully_literal() {
    let tokens = tokenize("write '/a b.txt'").unwrap();
    assert_eq!(tokens[1], Token::Word("/a b.txt".into()));
}

#[test]
fn single_quotes_do_not_process_backslash_escapes() {
    let tokens = tokenize(r"write '\n'").unwrap();
    assert_eq!(tokens[1], Token::Word(r"\n".into()));
}

#[test]
fn double_quotes_support_a_small_constrained_escape_set() {
    let tokens = tokenize(r#"write "line\nbreak\t\"quote\"\\""#).unwrap();
    assert_eq!(tokens[1], Token::Word("line\nbreak\t\"quote\"\\".into()));
}

#[test]
fn flag_with_inline_value() {
    let tokens = tokenize("write /a.txt --expected-revision=7").unwrap();
    assert_eq!(
        tokens[2],
        Token::Flag {
            name: "expected-revision".into(),
            value: Some("7".into())
        }
    );
}

#[test]
fn unterminated_single_quote_is_a_parse_error_not_a_hang() {
    assert_eq!(
        tokenize("write 'oops").unwrap_err(),
        LexError::UnterminatedQuote
    );
}

#[test]
fn unterminated_double_quote_is_a_parse_error() {
    assert_eq!(
        tokenize(r#"write "oops"#).unwrap_err(),
        LexError::UnterminatedQuote
    );
}

#[test]
fn nul_byte_is_rejected() {
    assert_eq!(tokenize("write /a\0b").unwrap_err(), LexError::NulByte);
}

#[test]
fn oversized_input_is_rejected_before_tokenizing() {
    let huge = "x".repeat(200_000);
    match tokenize(&huge) {
        Err(LexError::TooLong { max, actual }) => {
            assert_eq!(max, fslite_command::lexer::MAX_LINE_LEN);
            assert_eq!(actual, huge.len());
        }
        other => panic!("expected TooLong, got {other:?}"),
    }
}

#[test]
fn unquoted_shell_metacharacters_are_rejected_not_silently_literal() {
    for input in [
        "ls /a | rm /b",
        "ls /a; rm /b",
        "ls /a && rm /b",
        "ls /a > out",
        "ls `whoami`",
        "ls $(whoami)",
        "ls /a &",
    ] {
        assert!(
            matches!(tokenize(input), Err(LexError::UnsupportedMetacharacter(_))),
            "expected rejection for: {input}"
        );
    }
}

#[test]
fn metacharacter_immediately_after_an_empty_single_quote_is_rejected() {
    // An empty `''` contributes zero characters to the word, but a token
    // has still "started" — a metacharacter right after it must not be
    // treated as literal text just because the word string happens to be
    // empty at that point.
    for input in ["'';rm -rf /", "''|cat", "''&", "''<a", "''>a", "''`x"] {
        assert!(
            matches!(tokenize(input), Err(LexError::UnsupportedMetacharacter(_))),
            "expected rejection for: {input}"
        );
    }
}

#[test]
fn metacharacter_immediately_after_an_empty_double_quote_is_rejected() {
    assert!(matches!(
        tokenize("\"\";rm -rf /"),
        Err(LexError::UnsupportedMetacharacter(_))
    ));
}

#[test]
fn metacharacter_after_an_empty_quote_following_whitespace_is_rejected() {
    // Same bypass, but reached after a preceding whitespace-separated word,
    // to make sure the fix isn't accidentally position-dependent.
    assert!(matches!(
        tokenize("'a' '';b"),
        Err(LexError::UnsupportedMetacharacter(_))
    ));
}

#[test]
fn dollar_and_tilde_are_never_expanded_they_are_just_literal_bytes_inside_a_word() {
    // Not at a token boundary as a metacharacter trigger — embedded inside an
    // otherwise ordinary word, `$`/`~` are inert. This proves the lexer does
    // not special-case them for expansion anywhere, only rejects the
    // shell-substitution *forms* `$(...)`/backticks tested above.
    let tokens = tokenize("write /a$HOME~b.txt").unwrap();
    assert_eq!(tokens[1], Token::Word("/a$HOME~b.txt".into()));
}
