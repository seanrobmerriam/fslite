# fslite-core release notes

## Stability: Stable within 0.x

The `FileSystem` trait (28 methods), domain types, and stable typed errors
constitute the public API surface. Additive changes ship as minor bumps;
breaking changes (renaming/removing a method, changing an `*Options` field
that has no `#[serde(default)]`) trigger a minor bump per `SEMVER.md`.

## 0.1.0

Initial release. Every method on `FileSystem` is implemented; every
`ErrorCode` variant is stable; every `*Options` struct is
`#[non_exhaustive]`. See the root `CHANGELOG.md` for the complete list.
