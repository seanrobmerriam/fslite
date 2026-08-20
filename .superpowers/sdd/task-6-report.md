# Task 6: Server Container Image

## Implementation

Added a bounded Docker build context and a multi-stage server image. The image
builds only `fslite-server` with `rust:1.85-bookworm`, then runs its release
binary in `debian:bookworm-slim` as UID/GID `10001`. Runtime state is kept
under an owned `/data` volume through `FSLITE_DB=/data/fslite.db` and
`FSLITE_CONFIG=/data/server.json`; it binds `0.0.0.0:8080` for Compose-private
networking and exposes port 8080.

The runtime includes `ca-certificates` and `curl`, allowing a `/readyz`
health check. It contains no credential or credential default. The entrypoint
reports the configured database/configuration paths (but never a token),
checks their parent directories are writable, and `exec`s its command.

## Files

- `.dockerignore`
- `crates/fslite-server/Dockerfile`
- `crates/fslite-server/docker-entrypoint.sh`
- `crates/fslite-server/tests/container_smoke.sh`
- `crates/fslite-server/tests/container_contract.sh`

## RED/GREEN evidence

### RED

The smoke contract was added before the image artifacts, then run with its
default image name:

```text
sh crates/fslite-server/tests/container_smoke.sh
```

Docker was available and the command failed as expected because
`fslite-server:local` did not exist (`pull access denied`). This demonstrates
the contract required the not-yet-created image rather than passing against an
unrelated local image.

### GREEN/static verification

After adding the Dockerfile and entrypoint:

```text
sh -n crates/fslite-server/tests/container_smoke.sh
sh -n crates/fslite-server/docker-entrypoint.sh
cargo fmt --check
cargo check --workspace
cargo test -p fslite-server
git diff --check
```

All passed. The complete `fslite-server` suite passed: 1 library unit test, 28
binary unit tests, 96 integration tests, and doc tests (no failures).

Static container assertions also passed for the exact `.dockerignore` contract,
Rust 1.85 multi-stage builder, package-targeted release build, non-root user,
private-network bind, health check, writable-path entrypoint checks, and the
absence of a token assignment in Dockerfile/entrypoint.

## Docker build and runtime verification

BuildKit was unavailable because this Docker installation has no `buildx`
component:

```text
DOCKER_BUILDKIT=1 docker build -f crates/fslite-server/Dockerfile -t fslite-server:local .
ERROR: BuildKit is enabled but the buildx component is missing or broken.
```

The exact legacy build was therefore launched in a detached `screen` session
and polled from the terminal, avoiding the attached-command timeout:

```text
docker build -f crates/fslite-server/Dockerfile -t fslite-server:local .
Successfully built 0461d57172aa
Successfully tagged fslite-server:local

docker image inspect fslite-server:local --format '{{.Config.User}}'
10001:10001

sh crates/fslite-server/tests/container_smoke.sh fslite-server:local
exit=0
```

The smoke script passed its readiness, authenticated `/v1/me`, content write,
container recreation, and persisted-content read checks. It additionally
asserts that each successful startup log contains the non-secret configured
`FSLITE_DB` and `FSLITE_CONFIG` paths without printing container logs or token
values. Its cleanup was verified to leave no `fslite-server-smoke` container or
`fslite-server-smoke-data` volume.

Docker Desktop does not mount a single file from the system temporary directory
as a file in this environment, so the script's `mktemp -d` template now creates
its exact temporary directory beside the script, a Docker-shared location. The
required token-file mount and cleanup semantics are unchanged.

## Self-review

- The build context excludes Git/worktree metadata, local target output,
  showcase dependencies/build output, and SQLite database artifacts.
- `/data` is explicitly owned by UID/GID 10001 and declared as the persistent
  volume; both server state paths are below it.
- The smoke script uses only the named smoke container/volume and its exact
  `mktemp -d` directory. It refuses to remove pre-existing names, marks its
  container as cleanup-owned before `docker run`, and lets Docker fail
  naturally if port 18080 belongs to another process.
- The smoke flow waits for readiness, obtains `workspace_id` through an
  authenticated `/v1/me`, writes `persist.txt`, recreates only its container,
  and verifies the same named volume returns `persistent`.
- Credentials are supplied at runtime through the temporary read-only token
  file; no production credential appears in the image configuration.

## Review fixes and Rust 1.85 compatibility repair

### RED/GREEN for review fixes

`sh crates/fslite-server/tests/container_contract.sh` was added before the
entrypoint change and initially failed with:

```text
entrypoint did not report its configured FSLITE_DB path
```

After adding the two non-secret path lines, the same contract reached its
second regression and failed with:

```text
smoke cleanup did not remove a container created by failed docker run
```

Moving the cleanup-ownership marker before `docker run` made the complete
contract pass. The contract uses a deterministic temporary Docker stub: its
`run` command creates the exact smoke-container marker then fails, and the
test verifies the cleanup handler removes that marker and its volume marker.
It never supplies or prints a token.

### Rust 1.85 build blocker and repair

The detached exact image build initially completed with exit 101, rather than
being cut off by the command runner. Its concrete compiler output was 23
`E0658` errors, such as:

```text
error[E0658]: `let` expressions in this position are unstable
 --> crates/fslite-sqlite/src/content.rs:430:8
```

The affected source used `if let ... && ...` let-chain syntax, which Rust 1.85
does not support. With explicit approval for the necessary scope expansion,
the 23 expressions in `content.rs`, `directory.rs`, `mutate.rs`, `search.rs`,
and `trash.rs` were mechanically rewritten as equivalent nested `if` blocks.
No toolchain or behavior changed.

Verification after the rewrite:

```text
cargo fmt --check
cargo check --workspace
cargo test -p fslite-sqlite
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
sh crates/fslite-server/tests/container_contract.sh
```

All passed, followed by the successful exact Rust 1.85 image build and full
persistence smoke above.

## Concerns

None. The local `fslite-server:local` image is intentionally retained for the
showcase workflow; nothing was published, pushed, or deployed.
