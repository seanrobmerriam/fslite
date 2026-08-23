# Showcase Task 11 report

## RED / GREEN

- RED: the first real-browser journey targeted a standalone URL with no process
  fixture and failed with `net::ERR_CONNECTION_REFUSED`.
- GREEN: the isolated fixture starts the built Astro entry and a real
  `cargo run -p fslite-server` process, waits on readiness endpoints, and the
  original seeded-workspace journey passes.
- RED: the ordinary Vitest glob collected Playwright `e2e/*.spec.ts` files and
  failed because Playwright's `test()` was evaluated by Vitest.
- GREEN: Vitest now retains its default exclusions and additionally excludes
  `e2e/**`; the ordinary suite is again 31 files / 286 tests passing.

## Fixture and cleanup

- Every test gets an OS-temp `mkdtemp` fixture with a unique database,
  config, 0600 token file, loopback ports, and a unique bearer token.
- The Rust process is launched through `cargo run` under `RUSTUP_TOOLCHAIN=1.85.0`.
  The standalone Astro process receives the canonical private runtime
  configuration: `FSLITE_SERVER_URL`, `FSLITE_TOKEN_FILE`,
  `FSLITE_RESET_INTERVAL_MS`, `FSLITE_TRUST_PROXY`, and
  `FSLITE_REQUEST_TIMEOUT_MS`.
- Readiness is response-condition based; no fixed startup sleeps are used.
  Output is bounded to 24 KiB and redacts the token/private upstream hostname
  before it can appear in a startup failure diagnostic.
- Each detached child process group is PID-checked and terminated (TERM, then
  KILL only after a bounded exit wait); the fixture path is realpath-validated
  below the OS temporary directory before it is removed.
- After the full run, `pgrep` found no fixture Rust server, Astro entry, or
  Playwright worker process. Playwright's generated `test-results/` and report
  directories are ignored locally.

## Coverage

- Visible filesystem journey: folder/file creation, edit/save, upload/download,
  rename, move, copy, name/content search, trash/restore, permanent delete,
  purge, and Changes. After every visitor action, assertions verify the API
  activity method, upstream path fragment, and 200 status.
- Reset/concurrency: seeded startup and countdown, scheduled reseed generation,
  a real mutation race that observes the reset gate's 503 response, dirty draft
  preservation, and a second actor's stale-revision conflict with copy/reload
  recovery.
- Security/responsiveness: token/private-Docker-hostname absence from rendered
  DOM and same-origin response bodies; unknown-op and 1 MiB+1 upload rejection;
  actual 121-read, 31-mutation, and 11-upload limits; functional 375px tabs
  with no document horizontal overflow.

## Verification

- `rustup toolchain install 1.85.0 --profile minimal` installed
  `rustc 1.85.0 (4d91de4e4 2025-02-17)`.
- `PATH=/Users/sean/.nvm/versions/node/v22.12.0/bin:$PATH corepack pnpm --dir showcase test`:
  31 files / 286 tests passed.
- `PATH=/Users/sean/.nvm/versions/node/v22.12.0/bin:$PATH corepack pnpm --dir showcase check`:
  Astro, ESLint, and Prettier passed with no diagnostics.
- `PATH=/Users/sean/.nvm/versions/node/v22.12.0/bin:$PATH corepack pnpm --dir showcase build`:
  built the standalone Astro entry successfully.
- `PATH=/Users/sean/.nvm/versions/node/v22.12.0/bin:$PATH corepack pnpm --dir showcase test:e2e`:
  10/10 Playwright journeys passed against the real Rust 1.85 server.

## Commit

`test(showcase): verify real fslite journeys`

## Concerns

None. Chromium was already available locally; no browser-download fallback was
needed. `.DS_Store` files were preserved and not staged.

## Review follow-up

- Fixture setup now enters its top-level cleanup boundary immediately after
  `mkdtemp`. Validation, port allocation, token setup, child spawn, and either
  readiness failure therefore all run the same independent cleanup sequence.
  It validates the fixture path, attempts Astro then Rust detached-group
  shutdown, then removes only the validated directory; cleanup failures are
  aggregated after every attempt. Normal `ESRCH` exit races are tolerated.
- The harness rejects Node below 22.12 before any server process starts. This
  verification used Node `22.12.0`, Corepack pnpm `10.12.4`, and Rust `1.85.0`.
- Security coverage now awaits all captured same-origin response header/body
  reads and verifies bounded raw child output as well as redacted diagnostics
  contain neither the test token nor `fslite-server:8080`.
- Reset coverage now creates a pre-reset marker, verifies a real reset-gate
  503 race, waits for a generation change, refreshes the visible tree, and
  proves the marker is gone while the unsaved editor draft remains local.
- Final follow-up command: `corepack pnpm --dir showcase test:e2e` — 10/10
  real-server journeys passed; a subsequent process scan found no fixture
  Rust server, Astro entry, or Playwright worker.

## Lifecycle follow-up

- `freeLoopbackPort` now subscribes to both `listening` and `error` and always
  attempts listener close. Spawned Rust and Astro processes wait through an
  immediate spawn/error guard before readiness polling.
- `src/test/e2e-fixture.test.ts` verifies injected listener error cleanup and
  a real loopback allocation. The complete Vitest suite is now 32 files / 288
  tests; `test:e2e` remains 10/10.
- Mutation reconciliation refreshes in background mode so it does not append a
  second internal tree record after a visible visitor action. The responsive
  375px journey now verifies tree/editor/activity vertical order and creates a
  visible folder.

## Final reviewer follow-up

- `performAndAssertNewestActivity` snapshots `.api-activity .activity-list > li`
  before each visible request, requires exactly one new item, checks only that
  item for method/path/status, and verifies a UUID request id. The complete
  journey includes the otherwise easy-to-miss Save, archive creation, final
  renamed-file trash, restore, delete, purge, and Changes operations.
- Reset uses a transparent loopback proxy that byte-streams to real Rust and
  delays only reset responses. The test creates its marker through the UI,
  saves temporary README server text, preserves an editable local draft through
  the visible reset gate, and checks a fresh browser page against the exact
  `SEED_ENTRIES` README text.
- Conflict coverage grants real clipboard permissions and records two separate
  409 visitor PUTs. It checks copied local text before loading the exact second
  actor version. Public `revision_conflict` now consistently maps Rust's 412 to
  browser-facing 409, including the activity record.
- Fixture lifecycle primitives are exported and dependency-injected for unit
  coverage of spawn errors, bounded TERM/KILL exit handling, ESRCH races, and
  complete reverse-order cleanup after setup or cleanup failures. Diagnostics
  now has a redacted API in addition to raw bounded logs.
- Reset status polls separately from visitor activity so the browser can make
  reset state observable without a mutation or tree reconciliation; Trash
  restore/purge remove known items locally rather than appending a second list
  activity to the original visitor mutation.

## Final verification

- Node `22.12.0`, pnpm `10.12.4`: `pnpm test` — 33 files / 296 tests passed.
- `pnpm check` and `pnpm build` passed with no diagnostics.
- Real Rust `1.85` Playwright coverage passed in spec runs: Explorer 3/3,
  reset/conflict 2/2, security/responsive 6/6 (11/11 total; the new direct
  mutation/reconciliation regression is an additional journey).
