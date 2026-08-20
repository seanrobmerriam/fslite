# Task 5: Persistent Bootstrap and Usable Binary Wiring

## Result

Implemented persistent server startup in `fslite-server`. The binary now
resolves CLI/environment/stored configuration, opens a durable SQLite database,
selects or creates its default workspace, and serves it with one scoped bearer
credential.

## RED evidence

1. Added bootstrap contracts before implementation in
   `crates/fslite-server/src/server_bootstrap.rs`.
2. Ran `cargo test -p fslite-server --bin fslite-server server_bootstrap`.
   It failed as expected with missing `bootstrap`, `TokenSource`, and
   `ResolvedServerConfig::token_source` APIs.
3. Added a database-parent contract and ran
   `cargo test -p fslite-server --bin fslite-server missing_database_parent_is_created_before_opening_sqlite`.
   It failed as expected because SQLite could not open a database below a
   missing directory.

## GREEN evidence

- `cargo fmt --check`
- `cargo test -p fslite-server --bin fslite-server server_bootstrap` (7 passed)
- `cargo test -p fslite-server --test binary_bootstrap -- --nocapture` (1 passed)
- `cargo test -p fslite-server` (all unit, integration, and doc tests passed)
- `cargo clippy -p fslite-server --all-targets -- -D warnings`
- `cargo check --workspace`
- `cargo run -p fslite-server -- --help`

The help output includes `--db`, `--bind`, `--config`, `--token-file`, and all
three quota controls; it has no `--token` flag.

## Coverage

- Fresh paths create the database, workspace, state, and generated token.
- Restart retains database, workspace ID, and token.
- Environment/token-file sources are tracked separately from stored state, so
  a supplied process credential takes effect without overwriting the stored
  credential.
- A database without state preserves unrelated workspaces while receiving a
  new default workspace.
- Deleted stored workspaces are replaced and the replacement is persisted.
- Limits are used only to create a workspace; existing workspace quotas remain
  unchanged.
- Missing database parent directories are created before SQLite opens.
- The installed-binary smoke test verifies first-run/restart state, persisted
  file content, and authenticated `GET /v1/me`, with a child-process cleanup
  guard.

Exact first-run message coverage:

`No database or workspace found, creating default database and workspace`

The module contract asserts the exact message through `bootstrap_message()`;
the binary smoke test asserts it occurs exactly once on its first start and is
absent on restart.

## Files

- `crates/fslite-server/src/server_bootstrap.rs`
- `crates/fslite-server/src/main.rs`
- `crates/fslite-server/src/server_config.rs`
- `crates/fslite-server/tests/binary_bootstrap.rs`

`server_config.rs` gained token-source metadata solely to retain Task 4's
CLI/environment/stored precedence while preventing process overrides from
being persisted.

## Self-review and concerns

- Stored state remains atomically written with owner-only permissions from
  Task 4; token values remain redacted from debug/error paths.
- The only deliberate token display is the specified one-time fresh-token
  connection command. Later guidance uses `$FSLITE_TOKEN`.
- Startup errors are returned from `main`; child termination in the smoke test
  is guarded even if an assertion fails.
- No deployment or publishing was performed.

## Commit

`feat(server): bootstrap persistent default workspace`

## Review-fix evidence

### Findings addressed

1. The server now binds its `TcpListener` and reads `listener.local_addr()`
   before printing the bootstrap message, connection command, or listening
   line. `print_connection_guidance` receives that resolved address, so
   `--bind 127.0.0.1:0` advertises the actual reachable port and binding
   failures produce no success guidance.
2. The installed-binary test drains stdout and stderr on separate background
   readers. Its startup wait polls for early child exit and adds drained stderr
   to timeout/exit diagnostics, avoiding an undrained-stderr pipe deadlock.
   Diagnostics deliberately omit stdout because fresh-token guidance is
   intentionally printed there.

### RED/GREEN

- RED: strengthened the installed-binary contract to compare the address in
  the fresh-token connection command with the address from the listening line.
  `cargo test -p fslite-server --test binary_bootstrap -- --nocapture` failed
  as expected with advertised `127.0.0.1:0` versus the actual ephemeral port.
- GREEN: after binding before guidance and using `listener.local_addr()`, the
  same binary test passed.

### Review-fix verification

- `cargo fmt --check`
- `cargo test -p fslite-server --bin fslite-server server_bootstrap`
- `cargo test -p fslite-server --test binary_bootstrap -- --nocapture`
- `cargo test -p fslite-server`
- `cargo clippy -p fslite-server --all-targets -- -D warnings`
- `cargo check --workspace`
