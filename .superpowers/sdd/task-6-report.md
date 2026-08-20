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
only reports configured database/configuration paths, checks their parent
directories are writable, and `exec`s its command.

## Files

- `.dockerignore`
- `crates/fslite-server/Dockerfile`
- `crates/fslite-server/docker-entrypoint.sh`
- `crates/fslite-server/tests/container_smoke.sh`

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

## Docker validation limitation

The Docker daemon is available, and an isolated `rust:1.85-bookworm` build of
`cargo build --locked --release --package fslite-server` completed successfully
in 59.29 seconds. However, this environment terminates attached legacy
`docker build` clients after roughly 60 seconds. Each build reached the valid
Dockerfile compile layer and its builder container was cancelled at that
boundary (exit 101) before an image was produced. The daemon's logging driver
also does not expose failed builder logs. Therefore `docker image inspect` and
the image-backed persistence smoke flow could not be completed in this runner.
No image was published, pushed, or deployed.

## Self-review

- The build context excludes Git/worktree metadata, local target output,
  showcase dependencies/build output, and SQLite database artifacts.
- `/data` is explicitly owned by UID/GID 10001 and declared as the persistent
  volume; both server state paths are below it.
- The smoke script uses only the named smoke container/volume and its exact
  `mktemp -d` directory. It refuses to remove pre-existing names and lets
  Docker fail naturally if port 18080 belongs to another process.
- The smoke flow waits for readiness, obtains `workspace_id` through an
  authenticated `/v1/me`, writes `persist.txt`, recreates only its container,
  and verifies the same named volume returns `persistent`.
- Credentials are supplied at runtime through the temporary read-only token
  file; no production credential appears in the image configuration.

## Concerns

Image-build and image-backed smoke results remain unverified solely because of
the runner's 60-second legacy-Docker-client limit. Run the documented build
and `sh crates/fslite-server/tests/container_smoke.sh` in a normal Docker
environment to complete those two checks.
