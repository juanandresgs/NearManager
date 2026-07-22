# Near Bounded Task Runtime

`near-runtime` provides the backend-independent worker primitive used by interactive Near applications. It intentionally does not depend on Ratatui, Crossterm, a filesystem, or a particular async ecosystem.

## Guarantees

- A fixed worker count and bounded queue prevent unbounded thread or job growth.
- `spawn` never blocks the input thread; it returns `QueueFull` when capacity is exhausted.
- Every task receives a `CancellationToken`.
- Cancellation before execution produces `TaskOutcome::Cancelled`.
- A task that starts always returns its value, even after cooperative cancellation, so domain-specific exact summaries are preserved.
- Panics are isolated into `TaskOutcome::Panicked` rather than unwinding through the UI runtime.
- `TaskRecord` uses the same runtime task identity and owns running, completed, failed, and
  cancelled history transitions; application surfaces add titles, totals, and messages.
- Completion delivery uses a non-blocking channel and an installable wake callback. On macOS and
  Linux the callback wakes the terminal reactor immediately; completion latency does not depend on
  a periodic input poll.
- Dropping the pool cancels active tokens, closes workers, and joins them.

`near_runtime::block_on` parks a worker thread and uses a safe `Wake` implementation. It does not busy-poll pending provider futures.

## Workspace Integration

The Far workspace owns a two-worker pool with a bounded queue. The extracted workspace runtime
installs a reactor wake handle while running, drains completions before rendering and after wake
events, and clears the handle before terminal restoration.

Quick-view reads execute in background jobs. Moving the collection cursor cancels the previous task, issues a new generation ticket, and accepts completion only if both the task and viewer generation remain current.

Operation execution also runs in the pool behind a shared `OperationService`. Confirmation returns immediately, task history shows queued/running/final state, the Tasks surface can request cancellation, and cooperative cancellation preserves the operation engine's completed, skipped, failed, and pending summary.

Temporary-panel command capture registers the same runtime task in visible history before launch
and retains its final state after output is ingested. Copy-as-reference does not create an operation
task because it never plans or executes a resource mutation.

Provider listings use the same pool. Each panel owns a generation-scoped listing state; replacing it cancels every outstanding page and hydration handle. The first page replaces the visible collection, continuation pages append incrementally, and late completions can commit only when their panel, location, and generation still match.

Metadata hydration begins after each visible page and updates entries by `ResourceRef`. Per-item failures remain attached to `ResourceMetadata::field_errors` and do not discard usable page results. `near-fm` starts with empty collection surfaces and schedules both initial locations through this path, so launch does not synchronously enumerate either directory.
