//! Verb table: turns tokenized input (Task 3's [`crate::lexer`]) into a
//! typed [`Command`] (Task 1). One arm per verb in the coverage table.

use std::collections::HashMap;

use fslite_core::{
    ContentQuery, CopyOptions, CreateOptions, FindQuery, LinkTarget, MoveOptions, MutationOptions,
    NodeKind, PageRequest, ReadOptions, RemoveOptions, Revision, StatOptions, TouchOptions,
    TrashId, TreeOptions, VirtualPath, WriteOptions,
};

use crate::Command;
use crate::lexer::{LexError, Token, tokenize};

/// Why a line could not be parsed into a [`Command`].
#[derive(Debug, Eq, PartialEq)]
pub enum ParseError {
    Lex(LexError),
    UnknownVerb(String),
    MissingArgument {
        verb: &'static str,
        name: &'static str,
    },
    InvalidArgument {
        verb: &'static str,
        name: &'static str,
        reason: String,
    },
    UnknownFlag {
        verb: &'static str,
        flag: String,
    },
    /// More positional arguments were given than the verb accepts. Without
    /// this check, extras are silently discarded by index-based lookup
    /// (`Args::positional`) — e.g. `rm /a /b` would remove only `/a` and
    /// report success, with `/b` vanishing with no error or warning. This
    /// makes that fail loudly instead, matching the parser's design
    /// principle of never guessing intent (the same principle behind
    /// `check_known_flags` rejecting unknown flags).
    TooManyArguments {
        verb: &'static str,
        expected: usize,
        actual: usize,
    },
}

impl From<LexError> for ParseError {
    fn from(err: LexError) -> Self {
        ParseError::Lex(err)
    }
}

struct Args {
    positionals: Vec<String>,
    flags: HashMap<String, Option<String>>,
}

impl Args {
    fn positional(
        &self,
        verb: &'static str,
        index: usize,
        name: &'static str,
    ) -> Result<&str, ParseError> {
        self.positionals
            .get(index)
            .map(String::as_str)
            .ok_or(ParseError::MissingArgument { verb, name })
    }

    fn has_flag(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    fn flag_value(&self, name: &str) -> Option<&str> {
        self.flags.get(name).and_then(|v| v.as_deref())
    }

    fn check_known_flags(&self, verb: &'static str, known: &[&str]) -> Result<(), ParseError> {
        for flag in self.flags.keys() {
            if !known.contains(&flag.as_str()) {
                return Err(ParseError::UnknownFlag {
                    verb,
                    flag: flag.clone(),
                });
            }
        }
        Ok(())
    }

    /// Rejects more positional arguments than `expected`. Deliberately does
    /// *not* also reject too *few* — `Args::positional` already fails
    /// loudly with a specific `MissingArgument { verb, name }` for that case
    /// (naming exactly which argument is absent), which is more useful than
    /// a generic count mismatch and is relied on by existing callers/tests.
    /// This only closes the other gap: extras silently discarded past the
    /// last index the verb ever reads.
    fn check_positional_arity(
        &self,
        verb: &'static str,
        expected: usize,
    ) -> Result<(), ParseError> {
        if self.positionals.len() > expected {
            return Err(ParseError::TooManyArguments {
                verb,
                expected,
                actual: self.positionals.len(),
            });
        }
        Ok(())
    }

    fn expected_revision(&self, verb: &'static str) -> Result<Option<Revision>, ParseError> {
        match self.flag_value("expected-revision") {
            None => Ok(None),
            Some(raw) => {
                let value: u64 = raw.parse().map_err(|_| ParseError::InvalidArgument {
                    verb,
                    name: "expected-revision",
                    reason: "must be a non-negative integer".into(),
                })?;
                Revision::new(value)
                    .ok_or(ParseError::InvalidArgument {
                        verb,
                        name: "expected-revision",
                        reason: "must be nonzero".into(),
                    })
                    .map(Some)
            }
        }
    }

    fn page(&self, verb: &'static str) -> Result<PageRequest, ParseError> {
        let mut page = PageRequest::default();
        if let Some(cursor) = self.flag_value("cursor") {
            page = page.cursor(Some(cursor.to_string()));
        }
        if let Some(limit) = self.flag_value("limit") {
            let limit: u32 = limit.parse().map_err(|_| ParseError::InvalidArgument {
                verb,
                name: "limit",
                reason: "must be a non-negative integer".into(),
            })?;
            page = page.limit(limit);
        }
        Ok(page)
    }
}

fn split(tokens: Vec<Token>) -> (Vec<String>, HashMap<String, Option<String>>) {
    let mut positionals = Vec::new();
    let mut flags = HashMap::new();
    for token in tokens {
        match token {
            Token::Word(word) => positionals.push(word),
            Token::Flag { name, value } => {
                flags.insert(name, value);
            }
        }
    }
    (positionals, flags)
}

fn parse_path(
    verb: &'static str,
    name: &'static str,
    raw: &str,
) -> Result<VirtualPath, ParseError> {
    VirtualPath::parse(raw).map_err(|e| ParseError::InvalidArgument {
        verb,
        name,
        reason: e.message().to_string(),
    })
}

/// Parses one line of `fslite-command` grammar into a [`Command`].
pub fn parse(line: &str) -> Result<Command, ParseError> {
    let tokens = tokenize(line)?;
    let mut iter = tokens.into_iter();
    let verb_token = iter.next().ok_or(ParseError::MissingArgument {
        verb: "<line>",
        name: "verb",
    })?;
    let verb = match verb_token {
        Token::Word(w) => w,
        Token::Flag { name, .. } => return Err(ParseError::UnknownVerb(format!("--{name}"))),
    };
    let (positionals, flags) = split(iter.collect());
    let args = Args { positionals, flags };

    match verb.as_str() {
        "usage" => {
            args.check_known_flags("usage", &[])?;
            args.check_positional_arity("usage", 0)?;
            Ok(Command::WorkspaceUsage)
        }

        "stat" => {
            args.check_known_flags("stat", &["no-follow"])?;
            args.check_positional_arity("stat", 1)?;
            let path = parse_path("stat", "path", args.positional("stat", 0, "path")?)?;
            let options = StatOptions::default().follow_symlinks(!args.has_flag("no-follow"));
            Ok(Command::Stat { path, options })
        }

        "exists" => {
            args.check_known_flags("exists", &["no-follow"])?;
            args.check_positional_arity("exists", 1)?;
            let path = parse_path("exists", "path", args.positional("exists", 0, "path")?)?;
            let options = StatOptions::default().follow_symlinks(!args.has_flag("no-follow"));
            Ok(Command::Exists { path, options })
        }

        "ls" => {
            args.check_known_flags("ls", &["cursor", "limit"])?;
            args.check_positional_arity("ls", 1)?;
            let path = parse_path("ls", "path", args.positional("ls", 0, "path")?)?;
            Ok(Command::ReadDir {
                path,
                page: args.page("ls")?,
            })
        }

        "tree" => {
            args.check_known_flags("tree", &["max-depth", "follow-symlinks", "cursor", "limit"])?;
            args.check_positional_arity("tree", 1)?;
            let path = parse_path("tree", "path", args.positional("tree", 0, "path")?)?;
            let max_depth = args
                .flag_value("max-depth")
                .map(|v| {
                    v.parse().map_err(|_| ParseError::InvalidArgument {
                        verb: "tree",
                        name: "max-depth",
                        reason: "must be a non-negative integer".into(),
                    })
                })
                .transpose()?;
            let options = TreeOptions::default()
                .max_depth(max_depth)
                .follow_symlinks(args.has_flag("follow-symlinks"));
            Ok(Command::Tree {
                path,
                options,
                page: args.page("tree")?,
            })
        }

        "mkdir" => {
            args.check_known_flags("mkdir", &["parents", "exist-ok", "expected-revision"])?;
            args.check_positional_arity("mkdir", 1)?;
            let path = parse_path("mkdir", "path", args.positional("mkdir", 0, "path")?)?;
            let options = CreateOptions::default()
                .parents(args.has_flag("parents"))
                .exist_ok(args.has_flag("exist-ok"))
                .expected_revision(args.expected_revision("mkdir")?);
            Ok(Command::Mkdir { path, options })
        }

        "cat" => {
            args.check_known_flags("cat", &["range", "no-follow"])?;
            args.check_positional_arity("cat", 1)?;
            let path = parse_path("cat", "path", args.positional("cat", 0, "path")?)?;
            let range = args
                .flag_value("range")
                .map(|raw| {
                    let (start, end) = raw.split_once('-').ok_or(ParseError::InvalidArgument {
                        verb: "cat",
                        name: "range",
                        reason: "expected START-END".into(),
                    })?;
                    let start: u64 = start.parse().map_err(|_| ParseError::InvalidArgument {
                        verb: "cat",
                        name: "range",
                        reason: "invalid start".into(),
                    })?;
                    let end: u64 = end.parse().map_err(|_| ParseError::InvalidArgument {
                        verb: "cat",
                        name: "range",
                        reason: "invalid end".into(),
                    })?;
                    Ok::<_, ParseError>(fslite_core::ByteRange::new(start, end))
                })
                .transpose()?;
            let options = ReadOptions::default()
                .range(range)
                .follow_symlinks(!args.has_flag("no-follow"));
            Ok(Command::Read { path, options })
        }

        "write" => {
            args.check_known_flags("write", &["text", "no-create", "expected-revision"])?;
            args.check_positional_arity("write", 1)?;
            let path = parse_path("write", "path", args.positional("write", 0, "path")?)?;
            let bytes = args
                .flag_value("text")
                .map(|s| s.as_bytes().to_vec())
                .ok_or(ParseError::MissingArgument {
                    verb: "write",
                    name: "--text (or another payload source)",
                })?;
            let options = WriteOptions::default()
                .create(!args.has_flag("no-create"))
                .expected_revision(args.expected_revision("write")?);
            Ok(Command::Write {
                path,
                bytes,
                options,
            })
        }

        "write-at" => {
            args.check_known_flags(
                "write-at",
                &["offset", "text", "no-create", "expected-revision"],
            )?;
            args.check_positional_arity("write-at", 1)?;
            let path = parse_path("write-at", "path", args.positional("write-at", 0, "path")?)?;
            let offset: u64 = args
                .flag_value("offset")
                .ok_or(ParseError::MissingArgument {
                    verb: "write-at",
                    name: "--offset",
                })?
                .parse()
                .map_err(|_| ParseError::InvalidArgument {
                    verb: "write-at",
                    name: "offset",
                    reason: "must be a non-negative integer".into(),
                })?;
            let bytes = args
                .flag_value("text")
                .map(|s| s.as_bytes().to_vec())
                .ok_or(ParseError::MissingArgument {
                    verb: "write-at",
                    name: "--text",
                })?;
            let options = WriteOptions::default()
                .create(!args.has_flag("no-create"))
                .expected_revision(args.expected_revision("write-at")?);
            Ok(Command::WriteAt {
                path,
                offset,
                bytes,
                options,
            })
        }

        "append" => {
            args.check_known_flags("append", &["text", "expected-revision"])?;
            args.check_positional_arity("append", 1)?;
            let path = parse_path("append", "path", args.positional("append", 0, "path")?)?;
            let bytes = args
                .flag_value("text")
                .map(|s| s.as_bytes().to_vec())
                .ok_or(ParseError::MissingArgument {
                    verb: "append",
                    name: "--text",
                })?;
            let options =
                WriteOptions::default().expected_revision(args.expected_revision("append")?);
            Ok(Command::Append {
                path,
                bytes,
                options,
            })
        }

        "truncate" => {
            args.check_known_flags("truncate", &["length", "expected-revision"])?;
            args.check_positional_arity("truncate", 1)?;
            let path = parse_path("truncate", "path", args.positional("truncate", 0, "path")?)?;
            let length: u64 = args
                .flag_value("length")
                .ok_or(ParseError::MissingArgument {
                    verb: "truncate",
                    name: "--length",
                })?
                .parse()
                .map_err(|_| ParseError::InvalidArgument {
                    verb: "truncate",
                    name: "length",
                    reason: "must be a non-negative integer".into(),
                })?;
            let options =
                MutationOptions::default().expected_revision(args.expected_revision("truncate")?);
            Ok(Command::Truncate {
                path,
                length,
                options,
            })
        }

        "touch" => {
            args.check_known_flags("touch", &["no-create", "expected-revision"])?;
            args.check_positional_arity("touch", 1)?;
            let path = parse_path("touch", "path", args.positional("touch", 0, "path")?)?;
            let options = TouchOptions::default()
                .create(!args.has_flag("no-create"))
                .expected_revision(args.expected_revision("touch")?);
            Ok(Command::Touch { path, options })
        }

        "cp" => {
            args.check_known_flags("cp", &["recursive", "overwrite", "expected-revision"])?;
            args.check_positional_arity("cp", 2)?;
            let from = parse_path("cp", "from", args.positional("cp", 0, "from")?)?;
            let to = parse_path("cp", "to", args.positional("cp", 1, "to")?)?;
            let options = CopyOptions::default()
                .recursive(args.has_flag("recursive"))
                .overwrite(args.has_flag("overwrite"))
                .expected_revision(args.expected_revision("cp")?);
            Ok(Command::Copy { from, to, options })
        }

        "mv" => {
            args.check_known_flags("mv", &["overwrite", "expected-revision"])?;
            args.check_positional_arity("mv", 2)?;
            let from = parse_path("mv", "from", args.positional("mv", 0, "from")?)?;
            let to = parse_path("mv", "to", args.positional("mv", 1, "to")?)?;
            let options = MoveOptions::default()
                .overwrite(args.has_flag("overwrite"))
                .expected_revision(args.expected_revision("mv")?);
            Ok(Command::Move { from, to, options })
        }

        "rm" => {
            args.check_known_flags("rm", &["recursive", "expected-revision"])?;
            args.check_positional_arity("rm", 1)?;
            let path = parse_path("rm", "path", args.positional("rm", 0, "path")?)?;
            let options = RemoveOptions::default()
                .recursive(args.has_flag("recursive"))
                .expected_revision(args.expected_revision("rm")?);
            Ok(Command::Remove { path, options })
        }

        "ln" => {
            args.check_known_flags("ln", &["parents", "exist-ok", "expected-revision"])?;
            args.check_positional_arity("ln", 2)?;
            let target_raw = args.positional("ln", 0, "target")?;
            let link_raw = args.positional("ln", 1, "link")?;
            let target =
                LinkTarget::parse(target_raw).map_err(|e| ParseError::InvalidArgument {
                    verb: "ln",
                    name: "target",
                    reason: e.message().to_string(),
                })?;
            let link = parse_path("ln", "link", link_raw)?;
            let options = CreateOptions::default()
                .parents(args.has_flag("parents"))
                .exist_ok(args.has_flag("exist-ok"))
                .expected_revision(args.expected_revision("ln")?);
            Ok(Command::Symlink {
                target,
                link,
                options,
            })
        }

        "readlink" => {
            args.check_known_flags("readlink", &[])?;
            args.check_positional_arity("readlink", 1)?;
            let path = parse_path("readlink", "path", args.positional("readlink", 0, "path")?)?;
            Ok(Command::ReadLink { path })
        }

        "trash" => {
            args.check_known_flags("trash", &["expected-revision"])?;
            args.check_positional_arity("trash", 1)?;
            let path = parse_path("trash", "path", args.positional("trash", 0, "path")?)?;
            let options =
                MutationOptions::default().expected_revision(args.expected_revision("trash")?);
            Ok(Command::Trash { path, options })
        }

        "trash-ls" => {
            args.check_known_flags("trash-ls", &["cursor", "limit"])?;
            args.check_positional_arity("trash-ls", 0)?;
            Ok(Command::ListTrash {
                page: args.page("trash-ls")?,
            })
        }

        "restore" => {
            args.check_known_flags("restore", &["to", "expected-revision"])?;
            args.check_positional_arity("restore", 1)?;
            let raw_id = args.positional("restore", 0, "trash-id")?;
            let trash = TrashId::parse(raw_id).map_err(|_| ParseError::InvalidArgument {
                verb: "restore",
                name: "trash-id",
                reason: "not a valid id".into(),
            })?;
            let destination = args
                .flag_value("to")
                .map(|raw| parse_path("restore", "to", raw))
                .transpose()?;
            let options =
                MutationOptions::default().expected_revision(args.expected_revision("restore")?);
            Ok(Command::Restore {
                trash,
                destination,
                options,
            })
        }

        "purge" => {
            args.check_known_flags("purge", &[])?;
            args.check_positional_arity("purge", 1)?;
            let raw_id = args.positional("purge", 0, "trash-id")?;
            let trash = TrashId::parse(raw_id).map_err(|_| ParseError::InvalidArgument {
                verb: "purge",
                name: "trash-id",
                reason: "not a valid id".into(),
            })?;
            Ok(Command::Purge { trash })
        }

        "setattr" => {
            args.check_known_flags("setattr", &["value", "expected-revision"])?;
            args.check_positional_arity("setattr", 2)?;
            let path = parse_path("setattr", "path", args.positional("setattr", 0, "path")?)?;
            let key = args.positional("setattr", 1, "key")?.to_string();
            let value = args
                .flag_value("value")
                .map(|s| s.as_bytes().to_vec())
                .ok_or(ParseError::MissingArgument {
                    verb: "setattr",
                    name: "--value",
                })?;
            let options =
                MutationOptions::default().expected_revision(args.expected_revision("setattr")?);
            Ok(Command::SetAttribute {
                path,
                key,
                value,
                options,
            })
        }

        "rmattr" => {
            args.check_known_flags("rmattr", &["expected-revision"])?;
            args.check_positional_arity("rmattr", 2)?;
            let path = parse_path("rmattr", "path", args.positional("rmattr", 0, "path")?)?;
            let key = args.positional("rmattr", 1, "key")?.to_string();
            let options =
                MutationOptions::default().expected_revision(args.expected_revision("rmattr")?);
            Ok(Command::RemoveAttribute { path, key, options })
        }

        "glob" => {
            args.check_known_flags("glob", &["cursor", "limit"])?;
            args.check_positional_arity("glob", 1)?;
            let pattern = args.positional("glob", 0, "pattern")?.to_string();
            Ok(Command::Glob {
                pattern,
                page: args.page("glob")?,
            })
        }

        "find" => {
            args.check_known_flags(
                "find",
                &[
                    "name-contains",
                    "kind",
                    "min-size",
                    "max-size",
                    "modified-after",
                    "modified-before",
                    "cursor",
                    "limit",
                ],
            )?;
            args.check_positional_arity("find", 1)?;
            let root = parse_path("find", "root", args.positional("find", 0, "root")?)?;
            let kind = args
                .flag_value("kind")
                .map(|k| match k {
                    "file" => Ok(NodeKind::File),
                    "directory" => Ok(NodeKind::Directory),
                    "symlink" => Ok(NodeKind::Symlink),
                    other => Err(ParseError::InvalidArgument {
                        verb: "find",
                        name: "kind",
                        reason: format!("unknown kind `{other}`"),
                    }),
                })
                .transpose()?;
            let min_logical_size = args
                .flag_value("min-size")
                .map(|v| {
                    v.parse().map_err(|_| ParseError::InvalidArgument {
                        verb: "find",
                        name: "min-size",
                        reason: "must be a non-negative integer".into(),
                    })
                })
                .transpose()?;
            let max_logical_size = args
                .flag_value("max-size")
                .map(|v| {
                    v.parse().map_err(|_| ParseError::InvalidArgument {
                        verb: "find",
                        name: "max-size",
                        reason: "must be a non-negative integer".into(),
                    })
                })
                .transpose()?;
            let modified_after_ms = args
                .flag_value("modified-after")
                .map(|v| {
                    v.parse().map_err(|_| ParseError::InvalidArgument {
                        verb: "find",
                        name: "modified-after",
                        reason: "must be an integer".into(),
                    })
                })
                .transpose()?;
            let modified_before_ms = args
                .flag_value("modified-before")
                .map(|v| {
                    v.parse().map_err(|_| ParseError::InvalidArgument {
                        verb: "find",
                        name: "modified-before",
                        reason: "must be an integer".into(),
                    })
                })
                .transpose()?;
            let query = FindQuery::default()
                .root(root)
                .name_contains(args.flag_value("name-contains").map(str::to_string))
                .kind(kind)
                .min_logical_size(min_logical_size)
                .max_logical_size(max_logical_size)
                .modified_after_ms(modified_after_ms)
                .modified_before_ms(modified_before_ms);
            Ok(Command::Find {
                query,
                page: args.page("find")?,
            })
        }

        "grep" => {
            args.check_known_flags("grep", &["cursor", "limit"])?;
            args.check_positional_arity("grep", 2)?;
            let root = parse_path("grep", "root", args.positional("grep", 0, "root")?)?;
            let needle = args.positional("grep", 1, "needle")?.as_bytes().to_vec();
            let query = ContentQuery::default().root(root).needle(needle);
            Ok(Command::SearchContent {
                query,
                page: args.page("grep")?,
            })
        }

        "changes" => {
            args.check_known_flags("changes", &["after", "cursor", "limit"])?;
            args.check_positional_arity("changes", 0)?;
            let after = args
                .flag_value("after")
                .map(|raw| fslite_core::ChangeCursor::new(raw.to_string()));
            Ok(Command::Changes {
                after,
                page: args.page("changes")?,
            })
        }

        "batch" => {
            args.check_known_flags("batch", &["file"])?;
            args.check_positional_arity("batch", 0)?;
            let file = args.flag_value("file").ok_or(ParseError::MissingArgument {
                verb: "batch",
                name: "--file",
            })?;
            let contents =
                std::fs::read_to_string(file).map_err(|e| ParseError::InvalidArgument {
                    verb: "batch",
                    name: "file",
                    reason: e.to_string(),
                })?;
            let operations: Vec<fslite_core::BatchOperation> = serde_json::from_str(&contents)
                .map_err(|e| ParseError::InvalidArgument {
                    verb: "batch",
                    name: "file",
                    reason: e.to_string(),
                })?;
            Ok(Command::Batch(operations))
        }

        other => Err(ParseError::UnknownVerb(other.to_string())),
    }
}
