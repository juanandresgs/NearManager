# Generated Panels

Near models search output and extension resource collections as generated panels rather than copying resources into a synthetic namespace. Every row keeps its original `ResourceRef`, so view, edit, reveal, selection, and file-operation commands continue to dispatch through the source provider.

Generated panels are not Far Temporary Panels. They retain the output of a producer such as search or an extension; mutable Temporary Panels accept arbitrary references through the copy workflow and have ten independent slots. See `docs/near-temporary-panels.md`.

`near.search.keep-panel` retains the focused search or extension result panel in the workspace catalog. `near.search.panels` lists saved generated panels, and reopening one restores the exact source identities and the latest retained metadata. Search sessions and extension result sessions share the same catalog and remain available after navigating either panel elsewhere.

Refreshing a generated panel calls `stat` on every source through its registered provider. Successful results replace cached metadata. Missing providers, deleted resources, permission failures, and other source errors retain the row and mark it with `near.generated.stale = true`, `near.generated.stale-reason`, and a visible `stale — …` detail. This keeps vanished resources inspectable without presenting cached metadata as current. A later successful refresh clears the stale marker by replacing the metadata.

Extensions produce these panels with `ExtensionEffect::Open`. Empty collections report an explicit status instead of opening a blank panel. Initial rows use metadata already visible in either file panel when available; otherwise they carry a pending-refresh placeholder until the first refresh resolves the source provider.
