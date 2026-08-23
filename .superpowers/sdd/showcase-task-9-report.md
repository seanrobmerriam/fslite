# Showcase Task 9 Report

## RED / GREEN

- **RED:** New SearchPanel, TrashPanel, ChangesPanel, and ApiActivity contract
  suites first failed because the four panels did not yet exist. A new explorer
  integration failed because there was no accessible tablist, and a hook
  regression failed because there was no visitor-read operation path.
- **GREEN:** Search emits only `find { root, nameContains }`, `glob { pattern
  }`, and `search_content { root, text }`; validation blocks non-canonical
  roots and non-absolute/non-canonical globs before a request. Results provide
  a selectable matching tree path.
- **GREEN:** Trash lists on entry/explicit refresh, validates an optional
  restore destination, requires the exact current entry name for purge, and
  uses the existing one-mutation/one-tree-refresh guard for restore and purge.
- **GREEN:** Changes uses only the returned opaque `next_cursor` as `after`,
  ignores stale generation work, and deduplicates then sequence-sorts pages.
  API activity details redact sensitive header-shaped fields, bound display to
  the newest 100 records, expose JSON in `pre`, and copy only the sanitized
  curl. Clipboard success/failure is announced.

## Payload, tab, and activity security evidence

- Explorer, Search, Trash, and Changes are labelled tabs with `aria-controls`
  and labelled `tabpanel`s. Arrow keys/Home/End focus and automatically select
  a view; Enter/Space also select the focused tab. The editor remains in the
  wide Explorer workbench and the API activity region stays below every tab.
- Visitor discovery requests are busy/reset guarded, append exactly their
  returned activity, and never retry. Polling and reset/seed work continue to
  bypass browser-local activity. Existing dirty editor state and Task 8 dialog
  focus paths were retained.
- Built-client scan found no `fslite-server`, `fslite-client`, real bearer
  credentials, or reset/create/delete workspace calls. The only client curl
  placeholders are `$FSLITE_TOKEN` and `$FSLITE_SERVER_URL`; real bearer,
  token, server, upstream, internal, and header-shaped activity data is not
  rendered.

## Verification

```text
PATH=/Users/sean/.nvm/versions/node/v24.14.1/bin:$PATH corepack pnpm --dir showcase test
# 30 files, 257 tests passed

PATH=/Users/sean/.nvm/versions/node/v24.14.1/bin:$PATH corepack pnpm --dir showcase check
# Astro diagnostics: 0 errors, 0 warnings, 0 hints; ESLint and Prettier clean

PATH=/Users/sean/.nvm/versions/node/v24.14.1/bin:$PATH corepack pnpm --dir showcase build
# Astro SSR build completed

git diff --check
# passed
```

## Commit and concerns

- Commit: `feat(showcase): expose search trash changes and API activity`
- The existing untracked `.DS_Store` and `showcase/.DS_Store` files are
  preserved and deliberately excluded.
- No publish, deploy, reset invocation, or unrelated changes were made.
