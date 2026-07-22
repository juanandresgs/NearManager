# TUI Abstraction Harvest Evidence

- Near candidate: `406975c8e50c8a844bd603049ddd033d5d0eada7`
- NearTuiProof candidate: `29b6809`
- Proof repository: private `juanandresgs/NearTuiProof`

The proof depends only on `near-app` through the exact Near Git revision and contains no path dependency, Ratatui, Crossterm, `FarWorkspace`, or dual-panel assumption.

The candidate proof builds and tests these public contracts:

- Backend-neutral `ApplicationBuilder` and semantic snapshots.
- Custom non-filesystem provider and custom application surface.
- `CollectionSurface` and `CollectionViewport`, including `near.collection.page` behavior.
- Dialog, menu, help, task, settings, viewer, editor, and terminal surfaces.
- Provider-neutral `OperationPresentation`.

Validation command:

```text
CARGO_HOME=/tmp/near-proof-cargo python3 tools/validate_tui_proof.py \
  --repo /tmp/NearTuiProof \
  --revision 406975c8e50c8a844bd603049ddd033d5d0eada7
```

Result: PASS, including three proof-repository tests.

This evidence proves public consumption for the candidate. It does not replace direct terminal protocol evidence, settings persistence and rollback evidence, operation execution evidence, or operator workflow evidence; those remain governed by their dedicated qualification gates.
