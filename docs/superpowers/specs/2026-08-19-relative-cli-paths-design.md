# Relative CLI Paths Design

## Purpose

Make the `fslite` command line behave like a familiar filesystem tool. Users
may write `mkdir docs`, `touch docs/readme.md`, or `cat docs/readme.md`
without adding a leading slash. Relative virtual paths are resolved from the
active workspace root because fslite does not have a persistent virtual
current working directory.

Absolute virtual paths remain supported, so existing commands and scripts do
not break.

## Scope

The convenience applies uniformly to every virtual-path operand accepted by
the command parser:

- single paths such as those used by `stat`, `exists`, `ls`, `tree`, `mkdir`,
  `cat`, `write`, `write-at`, `append`, `truncate`, `touch`, `rm`, `readlink`,
  `trash`, `setattr`, and `rmattr`;
- both source and destination paths for `cp` and `mv`;
- the link location for `ln`;
- `restore --to`;
- the root operands for `find` and `grep`; and
- the pattern operand for `glob`.

The following values are deliberately outside this feature:

- `ln` targets retain their existing absolute-or-relative symlink semantics.
  For example, `ln ../target docs/link` stores `../target` unchanged while
  resolving the link location to `/docs/link`.
- Host filesystem paths, including `--db` and `batch --file`, remain host
  paths and are not rooted in the virtual workspace.
- Serialized `VirtualPath` values, the Rust API, HTTP requests, and batch JSON
  remain canonical and absolute.

## Architecture

Normalization occurs once in `fslite-command` at the text-command parser
boundary. The existing `parse_path` helper accepts either form:

- input beginning with `/` is passed to `VirtualPath::parse`;
- any other input is passed to `VirtualPath::root().join`.

Both branches produce the same canonical `VirtualPath`. Consequently local
execution, remote execution, the command codec, and filesystem backends keep
receiving absolute paths and need no changes.

`fslite-core::VirtualPath::parse` remains strict about absolute input. This
preserves the core type's invariant and prevents a CLI convenience from
silently changing the public Rust API or wire format.

## Normalization Rules

Path operands resolve from `/` and use the existing `VirtualPath`
normalization rules:

| CLI input | Canonical path |
| --- | --- |
| `docs` | `/docs` |
| `./docs` | `/docs` |
| `/docs` | `/docs` |
| `docs/../images` | `/images` |
| `../../docs` | `/docs` |

Traversal is clamped at the workspace root, matching current absolute-path
behavior. NUL bytes remain invalid.

Glob patterns follow the same root-relative rule while retaining wildcard
segments. The parser canonicalizes slash-separated `.` and `..` segments,
clamps traversal at the root, rejects NUL bytes, and ensures the pattern sent
to executors begins with `/`:

| CLI input | Canonical pattern |
| --- | --- |
| `docs/*.md` | `/docs/*.md` |
| `./docs/**/*.md` | `/docs/**/*.md` |
| `/docs/*.md` | `/docs/*.md` |

## Errors and Compatibility

Malformed paths still produce `ParseError::InvalidArgument` with the
originating verb and argument name. Existing absolute inputs produce the same
commands as before. The command codec continues serializing canonical
absolute paths, and remote servers receive no new path form.

No virtual current-directory state is introduced. Relative always means
workspace-root-relative in one-shot CLI use and in the REPL.

## User-Facing Documentation

The README quick start and named-workspace examples use relative paths to
show the simplest workflow. CLI help explains that virtual paths may be
absolute or workspace-root-relative. Glob help no longer says patterns must
be absolute and includes a relative example.

## Testing

Parser tests cover:

- relative and `./` single-path operands;
- both relative operands of a two-path command;
- relative `restore --to`, `find`, and `grep` roots;
- relative glob patterns and pattern normalization;
- preservation of relative `ln` targets;
- compatibility with absolute operands; and
- traversal clamping and NUL rejection.

An end-to-end bootstrap test creates `docs`, writes `docs/hello.txt`, and
reads it back using only relative paths. This proves the installed-style CLI
workflow works across parser, executor, SQLite backend, and persisted default
workspace behavior.

## Acceptance Criteria

- `fslite mkdir docs` succeeds in a new default workspace.
- `fslite write docs/hello.txt --text=hello` followed by
  `fslite cat docs/hello.txt` prints `hello`.
- All virtual-path operands accept workspace-root-relative input.
- Absolute input remains backward compatible.
- Core and wire-level paths remain canonical and absolute.
- Focused tests, the full workspace test suite, formatting, and Clippy pass.
