# FAR-SEARCH-003 — Search scopes, archives, links, and streams

## Implemented contract

- `ScopedSearchRequest` carries provider-qualified roots and independent archive, symbolic-link, and alternate-stream policies.
- Current, selected-directory, provider-advertised, and mounted-archive roots are resolved by the workspace without converting provider resources into native paths.
- Nested archives enter the same breadth-first traversal through `ProviderRegistry::mount`.
- Symbolic links use explicit skip, match, or follow behavior; unsupported follow requests emit capability diagnostics.
- Alternate streams use the provider-neutral `ResourceProvider::streams` contract and `resource.streams` capability. Unsupported providers emit one visible diagnostic per provider.
- Diagnostics remain attached to temporary search panels and are counted in completion status.

## Verification

- `near-search::tests::scoped_search_traverses_archives_and_reports_unsupported_streams` proves nested ZIP traversal preserves the archive provider identity and reports unsupported local alternate streams.
- `near-ui::workspace::tests::scoped_search_includes_archives_and_surfaces_stream_capability_diagnostics` proves selected, provider, and archive scope resolution plus visible workspace diagnostics.
- Existing recursive, append, refine, cancellation, advanced predicate, reveal, and saved-panel tests continue to pass through the scoped engine.
