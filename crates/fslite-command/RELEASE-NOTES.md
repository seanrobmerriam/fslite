# fslite-command release notes

## Stability: Preview

The `Command` codec is the contract for serializing `FileSystem`
operations across a transport. Verbs are stable; per-verb `*Options`
shape is stable. The `LocalExecutor` and `RemoteExecutor` interfaces
are stable. New sanitizers may be added without bumping the major.

## 0.1.0

Initial release. Typed `Command` codec (one variant per `FileSystem`
method); shell-like lexer/parser with no shell expansion;
`LocalExecutor` (in-process) and `RemoteExecutor` (HTTP) sharing the
`Executor` trait; three-tier terminal-output sanitizer
(`sanitize_name`, `sanitize_for_terminal`, `sanitize_preview`); the
`VERB_HELP` table consumed by `fslite help [<verb>]`. See the root
`CHANGELOG.md` for the complete list.
