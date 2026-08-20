# Release process

Crates in this workspace are versioned independently. A git tag may cover a
coordinated release, but it does not imply that every crate has the same
version. Publish production dependencies before their consumers, and wait for
crates.io to index each newly published dependency before publishing a crate
whose manifest requires it.

## Persistent server 0.2.0 release train

The persistent-server train changes only these package versions:

1. `fslite-sqlite 0.2.0`;
2. `fslite-server 0.2.0`, which requires `fslite-sqlite = "0.2.0"`;
3. `fslite 0.2.0`, whose manifest accepts `fslite-sqlite = "0.2.0"`.

`fslite-core 0.1.0` and `fslite-command 0.1.1` remain the already-published
production prerequisites for this train. Internal development dependencies are
path-only, so they do not add a publish-order requirement.

## Pre-publish checks

Run these commands from a clean checkout before publishing. They prepare and
validate packages only; they do not publish anything.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo publish --dry-run -p fslite-sqlite
git diff --check
```

After `fslite-sqlite 0.2.0` is indexed—but before publishing the dependent
crates—run:

```bash
cargo package -p fslite-server --allow-dirty --no-verify
cargo package -p fslite --allow-dirty --no-verify
```

Inspect the generated package file lists before continuing. They must include
the server's package-local `examples/server_and_remote_cli.rs` and relevant
release notes, and must not contain SQLite databases, credentials, `.env`
files, or build output. Do not represent a server publish dry run as complete
until `fslite-sqlite 0.2.0` is indexed on crates.io.

## Publish order and commands

After explicit release authorization, use this exact dependency order:

```bash
# 1. Publish the additive SQLite API.
cargo publish -p fslite-sqlite

# 2. Wait until crates.io indexes fslite-sqlite 0.2.0, then verify and publish
#    the server that requires it.
cargo publish --dry-run -p fslite-server
cargo publish -p fslite-server

# 3. Publish the CLI only when releasing its updated SQLite requirement.
cargo publish --dry-run -p fslite
cargo publish -p fslite
```

Publishing, tagging, pushing, image publication, and deployment are separate
authorized actions. Do not run any command in the last block during ordinary
development or release preparation. Once the authorized publishes have
succeeded, update the release record, create the agreed tag, and push it using
the repository's normal release approval process.

## Post-publish verification

Verify each published package and docs.rs page:

- https://docs.rs/fslite-sqlite
- https://docs.rs/fslite-server
- https://docs.rs/fslite
