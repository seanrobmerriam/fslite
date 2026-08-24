# fslite release notes

The CLI package is now published as `fslite` (formerly `fslite-cli`) so the
installation command and installed binary share one name.

## Stability: Preview

The CLI surface (flags, subcommands, verb grammar) is the contract for
shell-script embedders. Flag names and behavior are stable; new verbs
and flags may be added without bumping. Breaking changes will bump the
minor version per `SEMVER.md`.

## 0.2.0

The `fslite` package is released at `0.2.0` because its manifest now accepts
the additive `fslite-sqlite 0.2.0` release required by the persistent server
train. CLI behavior is otherwise unchanged; use it with `fslite-server`'s
first-run connection guidance or a protected `FSLITE_TOKEN` value.

## 0.1.1

Filesystem commands now accept workspace-root-relative paths, so installed
users can write `mkdir docs`, `write docs/hello.txt --text=hello`, and
`cat docs/hello.txt`. Absolute paths remain supported, and help text now
explains both forms.

## 0.1.0

Initial release. `fslite` binary with `--db`/`--memory`/`--server`
modes; `create`/`delete`/`use` subcommands for the local
filesystem/workspace registry; REPL mode; `--json` output; per-verb
help (`fslite help [<verb>]`); automatic persistent default creation on the
first ordinary filesystem command; atomic CLI state persistence. See the root
`CHANGELOG.md` for the complete list.
