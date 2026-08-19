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
   dependency order. **Internal-crate dev-dependencies are path-only**
   (no `version = "..."`) to break the `fslite-server` ↔ `fslite-command`
   cycle and to keep `fslite-sqlite` from gating on `fslite-conformance`:
   1. `fslite-core`        (no internal prod- or dev-deps)
   2. `fslite-conformance` (prod-deps: `fslite-core`)
   3. `fslite-sqlite`      (prod-deps: `fslite-core`)
   4. `fslite-command`     (prod-deps: `fslite-core`)
   5. `fslite-server`      (prod-deps: `fslite-core`, `fslite-sqlite`)
   6. `fslite`             (prod-deps: `fslite-command`, `fslite-core`, `fslite-sqlite`)

   `fslite-conformance`, `fslite-sqlite`, and `fslite-command` may be
   published in any relative order (they all only prod-dep on
   `fslite-core`). `fslite-server` requires `fslite-sqlite` to already
   be on crates.io; `fslite` requires both `fslite-command` and
   `fslite-sqlite` to already be on crates.io.
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
- https://docs.rs/fslite
