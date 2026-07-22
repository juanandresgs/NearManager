# Continuous quality evidence

## Performance

- `crates/near-testkit/tests/performance.rs` runs a deterministic 100×30 Far workspace with 100 warm-up iterations and 1,000 measured key-dispatch plus semantic-snapshot iterations, enforcing a 16 ms p95 ceiling.
- `crates/near-terminal/src/lib.rs` defines and tests the 50 ms idle poll contract used by application and workspace loops, with no animation-driven wakeup path.
- `.github/workflows/performance.yml` runs the regression gate on a schedule and on demand.
- `docs/near-performance.md` documents the workload, threshold, and idle behavior.

## Diagnostics

- `crates/near-core/src/diagnostic.rs` supplies schema-versioned correlated events, JSON export, version and capability metadata, and insertion-time redaction.
- `crates/near-ui/src/workspace.rs` correlates commands, tasks, providers, operations, plugins, and terminal sessions.
- Unit and workspace tests verify causal parentage, every required domain, export metadata, and sensitive-field removal.
- `docs/near-diagnostics.md` documents the correlation and privacy contracts.

## Accessibility

- `crates/near-ui/src/theme.rs` verifies monochrome focus, selection, combined selection/focus, warning, disabled, and selection-glyph distinctions.
- The same test module computes WCAG relative luminance and requires a 4.5:1 ratio for every text-bearing role in `specs/theme-high-contrast.toml`.
- Dialog, operation-preview, and task surfaces preserve validation, destructive safety, conflict, state, and progress information as text.
- `docs/near-accessibility.md` records the checklist and static reduced-motion behavior.
