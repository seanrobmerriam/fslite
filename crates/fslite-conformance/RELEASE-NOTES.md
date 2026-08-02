# fslite-conformance release notes

## Stability: Preview

The conformance suite is the contract every `FileSystem` implementation
must satisfy. Test names and case structure may shift as the canonical
contract evolves; pin to a specific version.

## 0.1.0

Initial release. `ConformanceFactory` trait + `run_conformance` driving
11 case groups (paths, directories, files, mutations, links, trash,
attributes, batches, search, changes, security) against any `FileSystem`
implementation. See the root `CHANGELOG.md` for the complete list.
