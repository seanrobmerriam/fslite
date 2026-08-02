# fslite-cli release notes

## Stability: Preview

The CLI surface (flags, subcommands, verb grammar) is the contract for
shell-script embedders. Flag names and behavior are stable; new verbs
and flags may be added without bumping. Breaking changes will bump the
minor version per `SEMVER.md`.

## 0.1.0

Initial release. `fslite` binary with `--db`/`--memory`/`--server`
modes; `create`/`delete`/`use` subcommands for the local
filesystem/workspace registry; REPL mode; `--json` output; per-verb
help (`fslite help [<verb>]`). See the root `CHANGELOG.md` for the
complete list.
