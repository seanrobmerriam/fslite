# fslite-server release notes

## Stability: Preview

The HTTP route table is part of the public contract, but the
`AuthProvider` trait and the `AppState` struct are still being hardened.
Breaking changes are likely before 1.0.

## 0.2.0

The `fslite-server` binary is now immediately usable after `cargo install`:
it persists a SQLite database, default workspace, and scoped bearer
credential across restarts. It binds localhost by default, has no plaintext
`--token` option, and supports `FSLITE_TOKEN`, `FSLITE_TOKEN_FILE`, and
`--token-file`; process-supplied credentials do not overwrite the stored
credential. Generated state is written atomically and is owner-only on Unix.

`GET /v1/me` exposes only the authenticated workspace ID and capabilities.
`POST /v1/workspaces/{workspace_id}/reset` requires that same workspace and
`workspace_admin`, and returns it to an empty state while retaining identity
and quotas. The included non-root Docker image persists `/data`, accepts
secret-file credentials, and is intended to sit behind a private server-side
gateway rather than receive browser traffic directly.

This additive server release depends on `fslite-sqlite 0.2.0`; publish that
crate first and wait for crates.io indexing before the server publish dry run
or release.

## 0.1.0

Initial release. `axum` HTTP adapter exposing `FileSystem` as a
resource-oriented API (`nodes`, `directories`, `trash`, `content`,
`search`, `batch`, `workspaces`); pluggable `AuthProvider`;
reference `BearerTokenAuthProvider`; `range::resolve_range` for HTTP
`Range:` header parsing; per-request `RequestId` correlation. See the root
`CHANGELOG.md` for the complete list.
