# fslite-sqlite release notes

## Stability: Stable within 0.x

The `SqliteFileSystem` implementation is the reference backend. The
public surface is the `FileSystem` trait inherited from `fslite-core`
plus the inherent `SqliteFileSystem::create_workspace` and
`SqliteFileSystem::delete_workspace` methods. Additive changes ship as
minor bumps; breaking changes trigger a minor bump per `SEMVER.md`.

## 0.1.0

Initial release. Every method on `FileSystem` is implemented over a
single SQLite database; multi-workspace isolation; per-workspace
`WorkspaceOptions`; recoverable trash; per-workspace change feed;
cursor-paginated listing/tree/search. `create_workspace` and
`delete_workspace` are inherent methods (not on `FileSystem`); they
are part of the public surface and follow the same stability commitment.
See the root `CHANGELOG.md` for the complete list.
