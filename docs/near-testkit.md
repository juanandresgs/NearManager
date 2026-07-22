# Near Deterministic Testkit

`near-testkit` exercises Near applications without a real terminal, wall clock, filesystem, or network provider.

## Contracts

- `ManualClock` advances only when a test requests it.
- `WorkflowHarness` drives normalized key, paste, and timeout events through a `FarWorkspace` at manual time.
- `ApplicationWorkflowHarness` drives any public `near_app::Application` through the same normalized
  key, paste, timeout, and semantic-capture protocol without importing file-manager internals.
- `WorkflowStep` and `GoldenFrame` define scripted workflows with named semantic snapshots.
- Every captured `SemanticSnapshot` contains both rendered cell content and semantic role IDs.
- `FakeProvider` schedules item, failure, and completion events against manual time.
- `GenerationGate` deterministically rejects events from superseded listing generations.
- Cancelling a fake request suppresses all later scripted events for that request.

The testkit intentionally models provider timing and generation behavior before the M1 provider implementation. The production provider API will consume the same generation, cancellation, and event semantics rather than adding nondeterministic sleeps to tests.

External application repositories should use `ApplicationWorkflowHarness`; `WorkflowHarness` is
reserved for assertions about Near's Far-compatible workspace policy.

## Interaction Conformance

`specs/interaction-conformance.toml` is the authoritative inventory for operator-visible input
behavior. A conformance case is not satisfied by command registration or model assertions alone. It
must identify the active context and key binding, the semantic command, cursor and selection state,
rendered visibility and roles, boundary behavior, an executable Rust test, and the operator scenario
that will provide direct terminal evidence.

Run the structural audit with:

```text
python3 tools/validate_interaction_conformance.py
```

Run the executable panel workflows with:

```text
cargo test -p near-ui panel_interaction_conformance
```

The production validator adds `--require-complete`. Implemented cases require passed exact-revision
operator evidence for their declared terminal matrix, while every discovery record must be complete.
The generated operator-session pack contains `operator-checklist.md` derived from the same cases.

For cursor-and-selection workflows, always assert the cursor and complete selected-resource set
independently. Render assertions must distinguish focused-selected, focused-unselected, and
selected-non-focused rows. Collections used for page tests must exceed the viewport, and mouse tests
must target rows after scrolling rather than only rows from the initial slice.
