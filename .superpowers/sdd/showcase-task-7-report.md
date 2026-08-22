# Showcase Task 7 Report

## RED / GREEN

- **RED:** Added role/name-first React Testing Library coverage for the file
  tree, editor keyboard save/download behavior, workspace countdown/limits,
  and reset/conflict controls. The initial focused run failed because the
  explorer component modules did not exist.
- **GREEN:** Implemented the one `ShowcaseExplorer` React island and reran the
  suite successfully. The final full Vitest run reports 21 files and 212 tests
  passing.

## Accessibility and design

- The tree uses `role="tree"` and button-backed `treeitem`s with roving focus,
  Arrow Up/Down, Right/Left expand/parent semantics, Home/End, Enter/Space,
  and the required level, set-size, position, expanded, and selected ARIA
  state.
- Editor, toolbar, status, notices, and conflict recovery all use named native
  controls; icon-only refresh has an accessible name and tooltip. Focus remains
  visibly outlined by the shared global focus treatment, and status/toasts use
  polite live regions.
- The workbench follows the approved restrained editorial/utilitarian
  direction: warm paper panels, crisp rules, slate ink, one blue accent,
  monospace only for paths and file data, and a narrow-screen stacked layout.
  Reduced-motion users receive no prolonged animation.

## Behavior and safeguards

- Status countdown derives from the server `now` value plus elapsed local time;
  it handles both a null schedule and reset-in-progress state. Public meters
  are fixed at 10 MiB and 250 nodes.
- Invalid UTF-8 and NUL-containing payloads are never decoded into the text
  editor; they expose metadata and download instead. Dirty editor text survives
  background refreshes and reset notices.
- Writes remain revision-aware. A conflict leaves unsaved text untouched until
  the visitor explicitly copies it or reloads the server version; reloading
  refreshes the tree first to use the current revision. Mutations are disabled
  and guarded in state while a reset or another mutation is active.

## Verification

Run with Node 26.7.0 and Corepack pnpm 10.12.4:

```text
corepack pnpm --dir showcase test     # 21 files, 212 tests passed
corepack pnpm --dir showcase check    # Astro, ESLint, Prettier all clean
corepack pnpm --dir showcase build    # SSR build completed
```

The built client bundle was scanned for server-only fslite client/config/token
references; none were present. The page contains exactly one primary hydrated
island: `<ShowcaseExplorer client:load />`.

## Commit and concerns

- Commit: `feat(showcase): browse and edit SQLite files`
- `showcase/` has no Playwright configuration or real-server fixture yet, so
  the browser end-to-end smoke belongs to the planned Task 11. The focused RTL
  keyboard coverage is the practical browser-level verification for this task.

## Review follow-up: draft safety and monotonic time

- Added RED/GREEN regressions for a delayed same-path invalid-UTF-8 response:
  dirty text, original baseline, and revision now remain intact and the UI
  exposes an explicit server-binary conflict. Clean invalid UTF-8 or NUL byte
  payloads still become download-only binary metadata without textarea
  corruption.
- Reset countdown now anchors server time to `performance.now()` after mount,
  with a `Date.now()` fallback only where monotonic time is unavailable. Its
  initial server-rendered value is deterministic, refreshed status snapshots
  re-anchor immediately, and wall-clock jumps cannot change elapsed progress.
- `useShowcase` now creates its default `ShowcaseApi` lazily and once per hook
  lifecycle rather than allocating it in a default parameter on every render.
- Added keyboard coverage for all required tree keys; Ctrl/Cmd+S validity and
  default prevention; both conflict actions; null/skew/reset countdown states;
  and reload after tree revision refresh.

Follow-up verification under Node 26.7.0 / Corepack pnpm 10.12.4:

```text
corepack pnpm --dir showcase test -- src/lib/browser src/components/explorer
# 21 files, 220 tests passed
corepack pnpm --dir showcase check
# Astro, ESLint, and Prettier clean
```
