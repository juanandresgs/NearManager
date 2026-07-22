# M1 Resource Runtime Evidence

Date: 2026-06-23

## Implemented Slice

- `FarWorkspace` owns independent generation-scoped listing state for its left and right collection surfaces.
- Starting a new listing cancels all outstanding page and hydration handles for the replaced generation without blocking input dispatch.
- Provider pages execute through the bounded `near-runtime` worker pool with a page size of 256.
- The first accepted page replaces the visible surface immediately; continuation pages append while preserving the current cursor model.
- Each accepted page schedules incremental `stat` hydration keyed by exact `ResourceRef` identity.
- Page completions can commit only when panel, location, request generation, and returned page generation remain current.
- Item metadata errors are recorded in `ResourceMetadata::field_errors`; page errors leave prior accepted entries usable and surface a non-fatal status.
- `near-fm` creates empty initial panels and schedules both startup locations through the asynchronous listing path.

## Automated Evidence

- `asynchronous_listing_rejects_a_stale_generation` starts a 100 ms listing, replaces it with a 5 ms listing in under 20 ms, and proves the obsolete late result cannot overwrite the new location.
- `paged_listing_renders_before_continuation_and_metadata_hydration` observes the first page while the continuation and delayed `stat` jobs remain pending, then verifies append and hydration completion.
- `listing_and_metadata_failures_preserve_usable_partial_results` verifies a failed continuation retains page-one entries and a failed item `stat` remains visible with a field-local error.
- Existing filesystem navigation and copy-refresh workflows now await the asynchronous pipeline and preserve their prior behavior.
- The local provider and collection tests retain cancellation, continuation, name-first listing, 100,000-entry virtualization, and warm render latency coverage.

## Requirement Status

- `REQ-RES-002` is verified. All acceptance clauses have direct deterministic tests and the production `near-fm` startup and navigation paths use the tested runtime.
