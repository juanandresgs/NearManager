# M1 Bounded Async Runtime Evidence

Date: 2026-06-23

## Implemented Slice

- `near-runtime` provides a bounded generic worker pool, cancellable handles, panic isolation, completion events, clean shutdown, and a wake-aware future executor.
- The workspace completion pump runs independently of rendering and input dispatch.
- Ctrl+Q quick-view reads execute off the input thread and commit only through current generation tickets.
- Operation execution runs off the input thread and publishes real task records to the Tasks surface.
- Task cancellation reaches `OperationEngine` and preserves exact partial summaries.
- Provider pages and per-page metadata hydration execute off the input thread and commit only to the matching panel location and listing generation.

## Automated Evidence

- Runtime unit tests prove ordinary completion, panic isolation, cooperative cancellation visibility, and correct wake/park behavior.
- A delayed provider test starts a 100 ms stale quick-view read, moves the cursor in under 20 ms, completes a newer 5 ms read, and proves the stale late completion cannot replace current content.
- A delayed operation test confirms execution dispatch returns in under 20 ms, cancels after execution begins, and records a cancelled task with one exact pending item.
- The real local copy workflow still previews, executes through the background runtime, refreshes provider panels, and records a completed task.
- Scripted slow-provider tests prove first-page rendering before continuation and metadata completion, stale-generation rejection, and non-fatal page and item-metadata failures.

## Requirement Status

- `REQ-VIEW-001` is verified: text and hex share bounded provider windows, large inputs remain within 64 KiB, and delayed quick-view loads are cancellable and stale-safe while input remains responsive.
- `REQ-OPS-002` is verified: operations execute off the input thread, expose task state, accept cancellation, preserve exact outcomes, support conflict scopes, and retain journal/retry evidence.
- `REQ-RES-002` is verified: listing is paged and cancellable, initial and navigated collections load asynchronously, generation checks reject stale completion, and partial results remain usable.

## Remaining Runtime Work

- Add richer per-item progress events for large local operations; current task progress is indeterminate while running and exact at completion.
