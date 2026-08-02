# Release process

This workspace uses a single git tag per release (`vX.Y.Z`) covering all
six crates at the same version.

## Cutting a release

1. Ensure `main` is green: `cargo fmt --all --check`,
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
   `cargo test --workspace --all-features`.
2. Update `CHANGELOG.md`: add a new section with the release date and a
   list of changes per crate.
3. Bump versions in every `crates/*/Cargo.toml` to the new version.
4. Run `cargo publish --dry-run -p <crate>` for each crate in
   dependency order:
   1. `fslite-core`
   2. `fslite-sqlite` (depends on `fslite-core`)
   3. `fslite-conformance` (depends on `fslite-core`)
   4. `fslite-server` (depends on `fslite-core`, `fslite-sqlite`)
   5. `fslite-command` (depends on `fslite-core`)
   6. `fslite-cli` (depends on `fslite-command`, `fslite-core`, `fslite-sqlite`)
5. Commit the version bumps and CHANGELOG entry.
6. Tag: `git tag -a vX.Y.Z -m "vX.Y.Z"`.
7. Push the tag: `git push origin vX.Y.Z`.
8. **Manual step (user only)**: `cargo login` and then `cargo publish`
   each crate in the same dependency order.

## Verifying the release

After publish, verify docs.rs renders every crate correctly:

- https://docs.rs/fslite-core
- https://docs.rs/fslite-sqlite
- https://docs.rs/fslite-conformance
- https://docs.rs/fslite-server
- https://docs.rs/fslite-command
- https://docs.rs/fslite-cli
