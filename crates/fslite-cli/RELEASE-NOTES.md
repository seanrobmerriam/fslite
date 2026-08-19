# fslite release notes

The CLI package is now published as `fslite` (formerly `fslite-cli`) so the
installation command and installed binary share one name.

## Stability: Preview

The CLI surface (flags, subcommands, verb grammar) is the contract for
shell-script embedders. Flag names and behavior are stable; new verbs
and flags may be added without bumping. Breaking changes will bump the
minor version per `SEMVER.md`.

## 0.1.0

Initial release. `fslite` binary with `--db`/`--memory`/`--server`
modes; `create`/`delete`/`use` subcommands for the local
filesystem/workspace registry; REPL mode; `--json` output; per-verb
help (`fslite help [<verb>]`); automatic persistent default creation on the
first ordinary filesystem command; atomic CLI state persistence. See the root
`CHANGELOG.md` for the complete list.
