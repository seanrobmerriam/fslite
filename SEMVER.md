# Semver policy

This workspace follows [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html)
with the following 0.x carve-out.

## 0.x (current)

All 0.x releases are pre-1.0. We commit to:

- **Any breaking change** to the public API of `fslite-core` or `fslite-sqlite`
  triggers at minimum a minor version bump (0.1 → 0.2).
- **Additive changes** (new methods, new options fields, new error-code
  variants) trigger at minimum a minor version bump.
- **Bug fixes and internal-only changes** trigger a patch version bump
  (0.1.0 → 0.1.1).
- **Per-crate stability tier** is declared in `crates/fslite-{core,sqlite,server,command,cli,conformance}/RELEASE-NOTES.md`
  and mirrored in the docs site (`reference/crates.md`).
- **Pre-1.0 changes that would be considered breaking in 1.x may be
  shipped as minor bumps in 0.x.** Embedders should pin to a specific
  minor version (`=0.1.0`) and review `CHANGELOG.md` on every bump.

## 1.0 (future)

The 1.0 cut requires:

- All 28 `FileSystem` methods stable for at least one minor release.
- All `fslite-server` HTTP routes stable for at least one minor release.
- Per-crate stability tier for every crate at "Stable within 0.x" or
  higher.
- `fslite-server`'s shipped binary deployable as-is (currently reference
  wiring only).
- A published security policy and a CVE disclosure process.

After 1.0, breaking changes require a major version bump.
