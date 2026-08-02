# fslite-server release notes

## Stability: Preview

The HTTP route table is part of the public contract, but the
`AuthProvider` trait and the `AppState` struct are still being hardened.
Breaking changes are likely before 1.0.

## 0.1.0

Initial release. `axum` HTTP adapter exposing `FileSystem` as a
resource-oriented API (`nodes`, `directories`, `trash`, `content`,
`search`, `batch`, `workspaces`); pluggable `AuthProvider`;
reference `BearerTokenAuthProvider`; `range::resolve_range` for HTTP
`Range:` header parsing; per-request `RequestId` correlation. The
shipped `main.rs` is reference wiring only; see the root `CHANGELOG.md`
for the complete list.
