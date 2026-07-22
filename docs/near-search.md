# Near Search Architecture

Near codifies search as a reusable resource operation rather than a file-manager dialog or a filesystem utility. The implementation is split into contracts that can be composed by any Near application.

## Contract Layers

1. `ResourcePredicate` is the single typed filter document for panel filtering, selection rules, operation scopes, and recursive searches.
2. `SearchService` traverses any hierarchical `ResourceProvider` using paged `list`, `stat`, and bounded `open` requests. `ScopedSearchRequest` carries provider-qualified roots, so one request can span unrelated providers without losing source identity.
3. `SearchEvent` streams bounded batches, progress, and capability diagnostics through a caller-selected delivery channel. Cancellation uses the shared `CancellationToken`.
4. `SearchResultsProvider` owns an incrementally populated `search://sessions/...` collection. Every listed item remains the original source `ResourceRef`.
5. Applications translate events into their surface model. `near-fm` appends batches to the focused collection while continuing to process keyboard input.

This boundary keeps policy, execution, collection identity, and presentation independently testable.

## Predicate Schema

The version-1 predicate document includes:

- name matching with exact, contains, glob, or validated regular-expression semantics;
- resource kinds, size bounds, and modification-time bounds;
- read-only and executable attribute predicates;
- explicit hidden-resource policy: exclude, include, or only;
- explicit ignore policy: none, version-control, or common generated directories;
- optional text, regular-expression, or hexadecimal content matching;
- explicit automatic, UTF-8, UTF-16LE, UTF-16BE, or Latin-1 decoding and case sensitivity.

New predicates carry `schema_version = 2`; version 1 documents remain readable for their original glob and literal-text semantics. Regex names and advanced content modes are rejected when mislabeled as version 1 so older semantics are never silently reinterpreted. Consumers validate the version before execution, and `matches_metadata` and `filters` remain the shared evaluation entry points for panel and operation contexts.

## Streaming and Cancellation

Traversal is breadth-first and page-bounded. Provider content reads are chunk-bounded; text and hexadecimal matching operate on decoded bytes, while regular-expression matching materializes the current resource so variable-length expressions remain correct across provider chunks. Search emits batches independently from progress, allowing a UI to render actionable results immediately.

Cancellation is checked before provider pages, resources, and content chunks. Already emitted batches are not rolled back. Provider errors stop future traversal but do not invalidate results already delivered to the collection.

## Scopes and Optional Resources

The Find Files dialog exposes four explicit scopes:

- `current` searches the focused panel root;
- `selected` searches selected directory resources, falling back to the current directory resource;
- `providers` searches every root advertised by registered providers;
- `archives` mounts selected archive resources and searches their provider-backed roots.

Nested archives are independently controlled with `exclude` or `include`. Inclusion uses `ProviderRegistry::mount`, so traversal is not coupled to ZIP or native paths. The breadth-first queue tracks provider and location together and suppresses repeated roots.

Symbolic links use an explicit `skip`, `match`, or `follow` policy. Follow is attempted only when the source provider advertises list capability for the link; otherwise a `search.follow-links` diagnostic is emitted. Alternate streams use `exclude` or `include` and the provider-neutral `ResourceProvider::streams` contract. Providers advertise support with `resource.streams`; unsupported requests emit visible diagnostics rather than silently changing policy.

`SearchEvent::Diagnostic` reports provider, location, required capability, and provider message. Near Manager displays diagnostics while searching, includes their count in completion status, and retains them with temporary result panels.

## Actionable Results

The search collection adds session metadata but never replaces source identity. View, edit, copy, move, inspect, and mutation commands therefore resolve through the source provider. Reveal asks the source provider for the result's parent and navigates there.

The collection provider itself advertises no source capabilities and does not impersonate the source provider. This avoids capability escalation and keeps command availability grounded in the real resource.

## Far Workflow

`Alt+F7` opens the recursive search dialog. One request composes scope, nested-archive policy, symbolic-link policy, alternate-stream policy, name mode, content mode, encoding, case policy, resource kinds, byte-size bounds, modification dates, read-only and executable attributes, hidden policy, and ignore policy. Sizes accept bytes or `K`, `M`, and `G` binary units; dates accept `YYYY-MM-DD` or Unix milliseconds. Invalid modes, ranges, dates, regexes, and hex byte strings remain in the dialog and report the responsible field. The mode field selects `replace`, `append`, or `refine`: replace starts a new result collection, append searches the original provider-qualified roots and adds only new source identities, and refine evaluates each result through its source provider. The focused panel becomes a `search://` collection immediately, batches append in the background, and `near.search.cancel` preserves received entries. `near.search.reveal` returns to the source parent.

`Ctrl+Shift+Alt+F7` keeps the current result collection as a session-persistent generated panel. Kept panels retain their provider-backed source references and continue receiving an in-flight search. `Ctrl+Alt+F7` lists kept generated panels and reopens the selected collection in the focused side. These are deliberately distinct from Far-compatible mutable Temporary Panels, which accept arbitrary references through F5 and use their own ten-slot lifecycle.

The same semantic commands remain available from the command palette and contextual help; the key is only a binding.

## Extension Points

- Add predicate nodes only through a new schema version or backward-compatible optional field.
- Add provider-specific ignore semantics through explicit policy adapters, not hidden global behavior.
- Add remote/server-side search by implementing a search executor that emits the same `SearchEvent` and source references.
- Add durable saved searches by persisting the predicate document plus root provider/location, never UI widget state.
- Add operation filtering by passing the same predicate to the operation scope builder before planning.
