use fslite_command::Command;
use fslite_command::parser::{ParseError, parse};
use fslite_core::{
    BatchOperation, ByteRange, ChangeCursor, ContentQuery, CopyOptions, CreateOptions, FindQuery,
    LinkTarget, MoveOptions, MutationOptions, NodeKind, PageRequest, ReadOptions, RemoveOptions,
    StatOptions, TouchOptions, TreeOptions, VirtualPath, WriteOptions,
};

fn path(s: &str) -> VirtualPath {
    VirtualPath::parse(s).unwrap()
}

// ---------------------------------------------------------------------
// One passing case per verb (28 verbs total).
// ---------------------------------------------------------------------

#[test]
fn usage_takes_no_arguments() {
    assert_eq!(parse("usage").unwrap(), Command::WorkspaceUsage);
}

#[test]
fn stat_defaults_follow_symlinks_true() {
    assert_eq!(
        parse("stat /a.txt").unwrap(),
        Command::Stat {
            path: path("/a.txt"),
            options: StatOptions::default()
        }
    );
}

#[test]
fn stat_no_follow_flag_disables_symlink_resolution() {
    assert_eq!(
        parse("stat /a.txt --no-follow").unwrap(),
        Command::Stat {
            path: path("/a.txt"),
            options: StatOptions::default().follow_symlinks(false)
        }
    );
}

#[test]
fn exists_defaults_follow_symlinks_true() {
    assert_eq!(
        parse("exists /a.txt").unwrap(),
        Command::Exists {
            path: path("/a.txt"),
            options: StatOptions::default()
        }
    );
}

#[test]
fn exists_no_follow_flag_disables_symlink_resolution() {
    assert_eq!(
        parse("exists /a.txt --no-follow").unwrap(),
        Command::Exists {
            path: path("/a.txt"),
            options: StatOptions::default().follow_symlinks(false)
        }
    );
}

#[test]
fn ls_takes_a_single_path_and_default_page() {
    assert_eq!(
        parse("ls /docs").unwrap(),
        Command::ReadDir {
            path: path("/docs"),
            page: PageRequest::default()
        }
    );
}

#[test]
fn ls_accepts_cursor_and_limit_flags() {
    assert_eq!(
        parse("ls /docs --cursor=abc --limit=10").unwrap(),
        Command::ReadDir {
            path: path("/docs"),
            page: PageRequest::default()
                .cursor(Some("abc".to_string()))
                .limit(10),
        }
    );
}

#[test]
fn ls_rejects_a_malformed_limit_instead_of_silently_dropping_it() {
    let err = parse("ls /docs --limit=abc").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "ls",
            name: "limit",
            ..
        }
    ));
}

#[test]
fn tree_reads_max_depth_and_follow_symlinks_flags() {
    assert_eq!(
        parse("tree /docs --max-depth=2 --follow-symlinks").unwrap(),
        Command::Tree {
            path: path("/docs"),
            options: TreeOptions::default()
                .max_depth(Some(2))
                .follow_symlinks(true),
            page: PageRequest::default(),
        }
    );
}

#[test]
fn tree_defaults_have_no_max_depth_and_no_follow() {
    assert_eq!(
        parse("tree /docs").unwrap(),
        Command::Tree {
            path: path("/docs"),
            options: TreeOptions::default(),
            page: PageRequest::default()
        }
    );
}

#[test]
fn tree_rejects_a_malformed_limit_instead_of_silently_dropping_it() {
    let err = parse("tree /docs --limit=abc").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "tree",
            name: "limit",
            ..
        }
    ));
}

#[test]
fn mkdir_parents_and_exist_ok_flags() {
    assert_eq!(
        parse("mkdir /docs --parents --exist-ok").unwrap(),
        Command::Mkdir {
            path: path("/docs"),
            options: CreateOptions::default().parents(true).exist_ok(true),
        }
    );
}

#[test]
fn cat_defaults_to_the_full_file() {
    assert_eq!(
        parse("cat /a.txt").unwrap(),
        Command::Read {
            path: path("/a.txt"),
            options: ReadOptions::default()
        }
    );
}

#[test]
fn cat_reads_the_range_flag() {
    assert_eq!(
        parse("cat /a.txt --range=0-10").unwrap(),
        Command::Read {
            path: path("/a.txt"),
            options: ReadOptions::default().range(Some(ByteRange::new(0, 10))),
        }
    );
}

#[test]
fn write_reads_the_literal_text_flag() {
    assert_eq!(
        parse(r#"write /a.txt --text="hello""#).unwrap(),
        Command::Write {
            path: path("/a.txt"),
            bytes: b"hello".to_vec(),
            options: WriteOptions::default()
        }
    );
}

#[test]
fn write_requires_exactly_one_payload_source() {
    let err = parse("write /a.txt").unwrap_err();
    assert!(matches!(
        err,
        ParseError::MissingArgument { verb: "write", .. }
    ));
}

#[test]
fn write_at_reads_offset_and_text() {
    assert_eq!(
        parse(r#"write-at /a.txt --offset=5 --text="hi""#).unwrap(),
        Command::WriteAt {
            path: path("/a.txt"),
            offset: 5,
            bytes: b"hi".to_vec(),
            options: WriteOptions::default(),
        }
    );
}

#[test]
fn write_at_requires_offset() {
    let err = parse(r#"write-at /a.txt --text="hi""#).unwrap_err();
    assert!(matches!(
        err,
        ParseError::MissingArgument {
            verb: "write-at",
            ..
        }
    ));
}

#[test]
fn append_reads_the_literal_text_flag() {
    assert_eq!(
        parse(r#"append /a.txt --text="more""#).unwrap(),
        Command::Append {
            path: path("/a.txt"),
            bytes: b"more".to_vec(),
            options: WriteOptions::default()
        }
    );
}

#[test]
fn truncate_reads_the_length_flag() {
    assert_eq!(
        parse("truncate /a.txt --length=10").unwrap(),
        Command::Truncate {
            path: path("/a.txt"),
            length: 10,
            options: MutationOptions::default()
        }
    );
}

#[test]
fn truncate_requires_length() {
    let err = parse("truncate /a.txt").unwrap_err();
    assert!(matches!(
        err,
        ParseError::MissingArgument {
            verb: "truncate",
            ..
        }
    ));
}

#[test]
fn touch_create_defaults_true_and_can_be_disabled() {
    assert_eq!(
        parse("touch /a.txt --no-create").unwrap(),
        Command::Touch {
            path: path("/a.txt"),
            options: TouchOptions::default().create(false)
        }
    );
}

#[test]
fn cp_takes_two_positional_paths() {
    assert_eq!(
        parse("cp /a /b --recursive --overwrite").unwrap(),
        Command::Copy {
            from: path("/a"),
            to: path("/b"),
            options: CopyOptions::default().recursive(true).overwrite(true),
        }
    );
}

#[test]
fn mv_takes_two_positional_paths() {
    assert_eq!(
        parse("mv /a /b --overwrite").unwrap(),
        Command::Move {
            from: path("/a"),
            to: path("/b"),
            options: MoveOptions::default().overwrite(true)
        }
    );
}

#[test]
fn rm_takes_a_path_and_recursive_flag() {
    assert_eq!(
        parse("rm /a --recursive").unwrap(),
        Command::Remove {
            path: path("/a"),
            options: RemoveOptions::default().recursive(true)
        }
    );
}

#[test]
fn ln_takes_a_target_and_a_link_path() {
    assert_eq!(
        parse("ln /target /link").unwrap(),
        Command::Symlink {
            target: LinkTarget::parse("/target").unwrap(),
            link: path("/link"),
            options: CreateOptions::default(),
        }
    );
}

#[test]
fn readlink_takes_a_single_path() {
    assert_eq!(
        parse("readlink /link").unwrap(),
        Command::ReadLink {
            path: path("/link")
        }
    );
}

#[test]
fn trash_accepts_an_optional_expected_revision() {
    assert_eq!(
        parse("trash /a.txt --expected-revision=3").unwrap(),
        Command::Trash {
            path: path("/a.txt"),
            options: MutationOptions::default().expected_revision(fslite_core::Revision::new(3)),
        }
    );
}

#[test]
fn trash_ls_takes_no_positionals() {
    assert_eq!(
        parse("trash-ls").unwrap(),
        Command::ListTrash {
            page: PageRequest::default()
        }
    );
}

#[test]
fn trash_ls_accepts_cursor_and_limit() {
    assert_eq!(
        parse("trash-ls --limit=5").unwrap(),
        Command::ListTrash {
            page: PageRequest::default().limit(5)
        }
    );
}

#[test]
fn trash_ls_rejects_a_malformed_limit_instead_of_silently_dropping_it() {
    let err = parse("trash-ls --limit=abc").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "trash-ls",
            name: "limit",
            ..
        }
    ));
}

#[test]
fn restore_takes_a_trash_id_and_defaults_destination_to_none() {
    let id = fslite_core::TrashId::new();
    let command = parse(&format!("restore {id}")).unwrap();
    assert_eq!(
        command,
        Command::Restore {
            trash: id,
            destination: None,
            options: MutationOptions::default()
        }
    );
}

#[test]
fn restore_accepts_a_to_destination_flag() {
    let id = fslite_core::TrashId::new();
    let command = parse(&format!("restore {id} --to=/elsewhere")).unwrap();
    assert_eq!(
        command,
        Command::Restore {
            trash: id,
            destination: Some(path("/elsewhere")),
            options: MutationOptions::default(),
        }
    );
}

#[test]
fn restore_rejects_a_malformed_trash_id() {
    let err = parse("restore not-a-uuid").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "restore",
            name: "trash-id",
            ..
        }
    ));
}

#[test]
fn purge_takes_a_trash_id() {
    let id = fslite_core::TrashId::new();
    let command = parse(&format!("purge {id}")).unwrap();
    assert_eq!(command, Command::Purge { trash: id });
}

#[test]
fn setattr_reads_path_key_and_value_flag() {
    assert_eq!(
        parse(r#"setattr /a.txt color --value="red""#).unwrap(),
        Command::SetAttribute {
            path: path("/a.txt"),
            key: "color".to_string(),
            value: b"red".to_vec(),
            options: MutationOptions::default(),
        }
    );
}

#[test]
fn setattr_requires_a_value_flag() {
    let err = parse("setattr /a.txt color").unwrap_err();
    assert!(matches!(
        err,
        ParseError::MissingArgument {
            verb: "setattr",
            ..
        }
    ));
}

#[test]
fn rmattr_reads_path_and_key() {
    assert_eq!(
        parse("rmattr /a.txt color").unwrap(),
        Command::RemoveAttribute {
            path: path("/a.txt"),
            key: "color".to_string(),
            options: MutationOptions::default(),
        }
    );
}

#[test]
fn glob_takes_a_pattern() {
    assert_eq!(
        parse("glob /*.txt").unwrap(),
        Command::Glob {
            pattern: "/*.txt".to_string(),
            page: Default::default()
        }
    );
}

#[test]
fn glob_rejects_a_malformed_limit_instead_of_silently_dropping_it() {
    let err = parse("glob /*.txt --limit=abc").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "glob",
            name: "limit",
            ..
        }
    ));
}

#[test]
fn find_reads_kind_and_name_contains_flags() {
    assert_eq!(
        parse("find / --kind=file --name-contains=report").unwrap(),
        Command::Find {
            query: FindQuery::default()
                .root(path("/"))
                .kind(Some(NodeKind::File))
                .name_contains(Some("report".to_string())),
            page: PageRequest::default(),
        }
    );
}

#[test]
fn find_rejects_an_unknown_kind() {
    let err = parse("find / --kind=bogus").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "find",
            name: "kind",
            ..
        }
    ));
}

#[test]
fn find_reads_size_and_modified_time_bounds() {
    assert_eq!(
        parse("find / --min-size=10 --max-size=100 --modified-after=1000 --modified-before=2000")
            .unwrap(),
        Command::Find {
            query: FindQuery::default()
                .root(path("/"))
                .min_logical_size(Some(10))
                .max_logical_size(Some(100))
                .modified_after_ms(Some(1000))
                .modified_before_ms(Some(2000)),
            page: PageRequest::default(),
        }
    );
}

#[test]
fn find_rejects_a_malformed_min_size_instead_of_silently_dropping_it() {
    let err = parse("find / --min-size=abc").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "find",
            name: "min-size",
            ..
        }
    ));
}

#[test]
fn find_rejects_a_malformed_max_size_instead_of_silently_dropping_it() {
    let err = parse("find / --max-size=abc").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "find",
            name: "max-size",
            ..
        }
    ));
}

#[test]
fn find_rejects_a_malformed_modified_after_instead_of_silently_dropping_it() {
    let err = parse("find / --modified-after=abc").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "find",
            name: "modified-after",
            ..
        }
    ));
}

#[test]
fn find_rejects_a_malformed_modified_before_instead_of_silently_dropping_it() {
    let err = parse("find / --modified-before=abc").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "find",
            name: "modified-before",
            ..
        }
    ));
}

#[test]
fn find_rejects_a_malformed_limit_instead_of_silently_dropping_it() {
    let err = parse("find / --limit=abc").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "find",
            name: "limit",
            ..
        }
    ));
}

#[test]
fn grep_reads_root_and_needle() {
    assert_eq!(
        parse("grep / needle").unwrap(),
        Command::SearchContent {
            query: ContentQuery::default()
                .root(path("/"))
                .needle(b"needle".to_vec()),
            page: PageRequest::default(),
        }
    );
}

#[test]
fn grep_rejects_a_malformed_limit_instead_of_silently_dropping_it() {
    let err = parse("grep / needle --limit=abc").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "grep",
            name: "limit",
            ..
        }
    ));
}

#[test]
fn changes_defaults_to_no_cursor() {
    assert_eq!(
        parse("changes").unwrap(),
        Command::Changes {
            after: None,
            page: PageRequest::default()
        }
    );
}

#[test]
fn changes_accepts_an_after_flag() {
    assert_eq!(
        parse("changes --after=cursor-123").unwrap(),
        Command::Changes {
            after: Some(ChangeCursor::new("cursor-123".to_string())),
            page: PageRequest::default(),
        }
    );
}

#[test]
fn changes_rejects_a_malformed_limit_instead_of_silently_dropping_it() {
    let err = parse("changes --limit=abc").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "changes",
            name: "limit",
            ..
        }
    ));
}

#[test]
fn batch_reads_operations_from_a_json_file() {
    let operations = vec![BatchOperation::Mkdir {
        path: path("/from-batch"),
        options: CreateOptions::default(),
    }];
    let json = serde_json::to_string(&operations).unwrap();
    let file = std::env::temp_dir().join(format!(
        "fslite-command-parser-test-batch-{}-{:?}.json",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&file, json).unwrap();

    let command = parse(&format!("batch --file={}", file.display())).unwrap();

    std::fs::remove_file(&file).ok();
    assert_eq!(command, Command::Batch(operations));
}

#[test]
fn batch_requires_a_file_flag() {
    let err = parse("batch").unwrap_err();
    assert!(matches!(
        err,
        ParseError::MissingArgument { verb: "batch", .. }
    ));
}

#[test]
fn batch_reports_an_invalid_argument_for_a_missing_file() {
    let err = parse("batch --file=/nonexistent/path/does-not-exist.json").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "batch",
            name: "file",
            ..
        }
    ));
}

// ---------------------------------------------------------------------
// Error cases.
// ---------------------------------------------------------------------

#[test]
fn unknown_verb_is_a_clear_error() {
    assert_eq!(
        parse("frobnicate /a").unwrap_err(),
        ParseError::UnknownVerb("frobnicate".to_string())
    );
}

#[test]
fn unknown_flag_is_a_clear_error() {
    let err = parse("stat /a.txt --bogus").unwrap_err();
    assert!(matches!(err, ParseError::UnknownFlag { verb: "stat", flag } if flag == "bogus"));
}

#[test]
fn missing_required_positional_is_a_clear_error() {
    let err = parse("stat").unwrap_err();
    assert!(matches!(
        err,
        ParseError::MissingArgument {
            verb: "stat",
            name: "path"
        }
    ));
}

// ---------------------------------------------------------------------
// Positional arity: extra arguments must fail loudly, not be silently
// discarded by index-based lookup.
// ---------------------------------------------------------------------

/// Live-verified bug: `rm /a /b` used to succeed (exit 0) and remove only
/// `/a`, with `/b` silently discarded — no error, no warning. This is a
/// real data-safety footgun for a destructive verb, and directly
/// contradicts the parser's design principle that unexpected input must
/// fail loudly rather than have its intent guessed (the same principle that
/// makes an unknown flag an error via `check_known_flags`).
#[test]
fn rm_with_an_extra_positional_argument_is_a_clear_error_not_a_silent_partial_removal() {
    let err = parse("rm /a /b").unwrap_err();
    assert!(
        matches!(
            err,
            ParseError::TooManyArguments {
                verb: "rm",
                expected: 1,
                actual: 2
            }
        ),
        "expected a TooManyArguments error, got {err:?}"
    );
}

/// Same bug, for a read-only verb (`stat /b /nonexistent /garbage`): every
/// extra positional past the first used to vanish silently instead of being
/// reported.
#[test]
fn stat_with_extra_positional_arguments_is_a_clear_error() {
    let err = parse("stat /b /nonexistent /garbage").unwrap_err();
    assert!(
        matches!(
            err,
            ParseError::TooManyArguments {
                verb: "stat",
                expected: 1,
                actual: 3
            }
        ),
        "expected a TooManyArguments error, got {err:?}"
    );
}

/// A two-positional verb (`cp`) with a third, unexpected argument must also
/// be rejected — proves the arity check is wired for two-positional verbs,
/// not just the one-positional case above.
#[test]
fn cp_with_an_extra_positional_argument_is_a_clear_error() {
    let err = parse("cp /a /b /c").unwrap_err();
    assert!(
        matches!(
            err,
            ParseError::TooManyArguments {
                verb: "cp",
                expected: 2,
                actual: 3
            }
        ),
        "expected a TooManyArguments error, got {err:?}"
    );
}

/// The correct-arity case for both a one-positional and a two-positional
/// verb must still parse successfully — the arity check must not be
/// over-eager and reject exactly the expected count.
#[test]
fn correct_arity_still_parses_successfully_for_one_and_two_positional_verbs() {
    assert!(parse("rm /a").is_ok());
    assert!(parse("cp /a /b").is_ok());
}

/// A zero-positional verb (`usage`) with a stray extra word must also be
/// rejected, not silently ignored.
#[test]
fn zero_positional_verb_with_a_stray_extra_word_is_a_clear_error() {
    let err = parse("usage extra").unwrap_err();
    assert!(
        matches!(
            err,
            ParseError::TooManyArguments {
                verb: "usage",
                expected: 0,
                actual: 1
            }
        ),
        "expected a TooManyArguments error, got {err:?}"
    );
}

#[test]
fn invalid_expected_revision_is_a_clear_error() {
    let err = parse("trash /a.txt --expected-revision=0").unwrap_err();
    assert!(matches!(
        err,
        ParseError::InvalidArgument {
            verb: "trash",
            name: "expected-revision",
            ..
        }
    ));
}

#[test]
fn empty_line_is_a_missing_verb_error() {
    assert!(parse("").is_err());
    assert!(parse("   ").is_err());
}

#[test]
fn lexer_errors_propagate_as_parse_errors() {
    assert!(matches!(
        parse("write 'unterminated"),
        Err(ParseError::Lex(_))
    ));
}
