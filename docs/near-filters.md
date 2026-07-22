# Near Panel Filters

Near models reusable panel filters as provider-neutral metadata predicates. A filter catalog is layered through `filters.toml`, so the same named filters and mask groups can be reused across local, archive, search-result, and extension providers that expose compatible metadata.

## Catalog Model

`[[mask_groups]]` assigns a stable ID and label to one or more glob masks. A `[[filters]]` entry has a stable ID, label, mode, optional `mask_group`, and an optional `ResourcePredicate` table.

Modes compose in this order:

1. `force-exclude` rejects a matching resource;
2. `force-include` accepts a matching resource;
3. `exclude` rejects a matching resource;
4. when ordinary `include` filters are active, at least one must match.

Predicates support names, kinds, minimum and maximum sizes, modified-time bounds, read-only and executable state, hidden state, and ignore state. Content predicates are rejected because panel filtering must remain bounded and metadata-driven. A filter may use either a named mask group or an inline name predicate, not both.

## Runtime Workflow

Press `Ctrl+Shift+F` or choose **Filters** from the F9 menu. The menu displays each filter's mode marker and active state. Toggling a filter refreshes both panels, but applies the filter only to the focused panel. The panel border gains `*` while any filter is active.

Filter state is independent per panel and survives navigation during the current workspace session. **Clear current panel filters** disables every active filter for the focused panel.

When filters need size, date, or attribute fields, Near hydrates listing entries with provider `stat` calls on background workers before evaluating them. Metadata failures are attached to the entry instead of blocking the UI thread.

## Configuration

The shipped example is `specs/filters.toml`. Override it with `--filters FILE`, `NEAR_FILTERS`, the user configuration root, or trusted `.near/filters.toml` using the standard layered precedence rules.
