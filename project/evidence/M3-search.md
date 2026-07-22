# M3 Search Evidence

Date: 2026-06-23

## Implemented Slice

- `near-search` defines the versioned `ResourcePredicate`, exact/contains/glob/regex names, text/regex/hex content with explicit encodings, kinds, size and date ranges, permission attributes, explicit hidden and ignore policies, and provider-neutral recursive search.
- The search engine traverses paged providers, performs chunk-bounded provider reads, emits bounded result batches and progress, and checks shared cancellation throughout traversal.
- `SearchResultsProvider` incrementally exposes `search://sessions/...` while returning each result's exact source `ResourceRef` and stable metadata.
- `near-app` reexports the search contracts as part of the backend-independent application facade.
- `near-fm` implements `Alt+F7`, a semantic search dialog, streamed panel updates, cancellation, view/edit routing, canonical operation targets, reveal-to-source-parent, repeated append/refine modes, and session-persistent result panels.

## Automated Evidence

- Predicate equivalence tests prove panel filtering and operation-scope filtering produce the same ordered source set.
- `predicate-v1.toml` proves explicit hidden/ignore policies and versioned serialization round trips.
- Recursive content-search tests prove source identity, bounded batches, cancellation retention, composed metadata criteria, UTF-16 regex matching, hexadecimal byte matching, and field-specific validation.
- Search-provider tests prove paged collections emit the exact source reference rather than a synthetic search item identity.
- The Far workflow test proves search launch is non-blocking, streamed results remain local-provider resources, view and edit commands resolve normally, canonical copy targets are source resources, and reveal navigates through the source provider.
- Shipped-keymap validation proves `near.search.start` is registered and `Alt+F7` remains a configurable binding rather than a command identity.

## Requirement Status

- `REQ-SEARCH-001` is verified by the shared predicate evaluator, advanced criteria, explicit policies, schema fixture, validation diagnostics, and equivalence tests.
- `REQ-SEARCH-002` is verified by provider-neutral streaming, cancellation retention, source-backed collection identity, responsive Far integration, and normal resource-command routing.
- `WF-SEARCH-001` is covered by the recursive Far workspace workflow test.
