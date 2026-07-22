# Near Architecture and Efficiency Review

Date: 2026-07-04  
Scope: current `autonomy/wave-1-production-qualification` worktree, including uncommitted interaction, presentation, and idle-runtime corrections.

## Executive conclusion

Near has a sound **architectural thesis**, but the current implementation is not yet the smartest or simplest realization of it.

The strongest parts are the provider-neutral resource contracts, semantic commands, typed operation safety, terminal restoration, bounded task queue, semantic rendering boundary, and unusually strong qualification infrastructure. Those are appropriate improvements over Far Manager rather than gratuitous abstractions.

The principal weakness is that application composition, Far policy, interaction state, asynchronous coordination, persistence, rendering, and feature integration have accumulated in one `FarWorkspace` coordinator. That type currently has 91 fields, its source file is 20,074 lines, and the file contains approximately one third of the repository's 60,511 Rust source lines. The declared model/update/effect architecture therefore exists more clearly in the design records than in the running file-manager implementation.

The result is functionally broad but not yet Far-like in the engineering sense of **small hot paths, low ceremony, pay-for-use features, direct state transitions, and near-zero idle cost**. Recent discoveries—missing navigation semantics, incomplete selection behavior, indistinct resource presentation, and terminal loops consuming a core—were not unrelated defects. They are symptoms of the same structural condition: interaction invariants are distributed across a very large coordinator and qualification proves individual declared cases more readily than it proves the complete behavioral grammar.

The recommended direction is not a rewrite and not more framework. It is a controlled simplification:

1. Make one explicit message/update/effect state machine the only owner of mutable application state.
2. Replace 50 ms completion polling with a wakeable reactor that blocks until terminal input, a task completion, a signal, or an actual timer deadline.
3. Split `FarWorkspace` by state ownership and command domains while keeping the user-visible application intact.
4. Make archive, SFTP, embedded PTY, and Wasm hosting pay-for-use capabilities rather than unconditional core-binary weight.
5. Cache stable view models and render work proportional to the visible viewport and changed semantic regions.
6. Qualify the interaction grammar from an external Far-compatible oracle, not only from implementation-authored examples.

## Evidence classification

### Direct code-backed findings

- The intended product principles explicitly prioritize keyboard certainty, stable spatial context, visible affordances, current-item-plus-selection behavior, terminal continuity, low ceremony, and progressive extensibility (`docs/near-platform-blueprint.md:39`).
- The intended runtime is a single-owner event/update/effect loop with asynchronous services (`project/architecture/README.md:48`, `project/architecture/README.md:93`).
- `FarWorkspace` begins at `crates/near-ui/src/workspace.rs:503` and currently contains 91 fields spanning panels, editors, viewer state, overlays, keyboard state, command line, filters, persistence stores, providers, operations, themes, external tools, removable devices, quick view, tasks, searches, macros, extensions, PTY state, diagnostics, and generations.
- `crates/near-ui/src/workspace.rs` is 20,074 lines. The whole Rust source tree is 60,511 lines; `near-ui` alone is 35,484 lines. These measurements were produced with `find ... | xargs wc -l` on this worktree.
- The generic `near-ui` crate directly depends on configuration, operations, runtime, search, macros, handlers, terminal, and optional PTY integration (`crates/near-ui/Cargo.toml:12`). The intended `near-shell` layer shown in the blueprint is not a distinct implementation boundary (`docs/near-platform-blueprint.md:70`).
- The current workspace starts two worker threads and a bounded queue of 32 immediately (`crates/near-ui/src/workspace.rs:692`). The task implementation itself is bounded and cancellable (`crates/near-runtime/src/lib.rs:86`).
- Both application loops still wake at most every 50 ms to inspect signals, keymap deadlines, and background completions (`crates/near-terminal/src/lib.rs:14`, `crates/near-ui/src/application.rs:320`, `crates/near-ui/src/workspace.rs:12014`). Rendering is now invalidation-driven, which removed the continuous redraw bug, but wake-up scheduling is still polling-based.
- The idle regression measured approximately 0.96% of one core for `near-fm` and 0.89% for `near-view`, with terminal-hangup exit around 0.26 seconds (`.near/qualification/logs/idle-cpu.log`). This is a substantial correction from the runaway behavior but is not yet the near-zero wake profile expected from a fully event-driven terminal program.
- The interaction performance gate measures only repeated `Down` dispatch plus a semantic snapshot against `FarWorkspace::demo()` at 100×30 (`crates/near-testkit/tests/performance.rs:14`). It proves that specific p95 is below 16 ms; it does not establish startup time, resident memory, allocation rate, live-terminal presentation latency, large-directory mutation cost, task-completion latency, or plugin overhead.
- `near-fm` has 188 unique transitive dependency packages in the current Cargo graph and its existing release binary is 22 MiB. `near-view` is 1.9 MiB. These values were measured from `cargo tree --offline -p near-fm` and `target/release` in this worktree.
- `near-fm` unconditionally links the plugin host, archive support, SFTP, embedded PTY, and their dependencies (`apps/near-fm/Cargo.toml:8`). `near-plugins` unconditionally includes Wasmtime and Cranelift support (`crates/near-plugins/Cargo.toml:15`). Wasmtime symbols are present in the release binary.
- The public `near-app` facade re-exports a large cross-section of configuration, core, handlers, macros, operations, runtime, search, terminal, and UI types (`crates/near-app/src/lib.rs:8`). This is backend-neutral at the type level, but broad enough that application consumers can couple to many implementation concepts at once.
- The provider contract itself is appropriately small and backend-neutral (`crates/near-core/src/provider.rs:110`). However, archive and SFTP implementations import `LocalFileProvider` in production code (`crates/near-archive/src/lib.rs:19`, `crates/near-sftp/src/lib.rs:19`), so cross-provider implementation boundaries are not fully neutral.
- The current qualification result passes 394 tests with zero failures, while the recent operator session still revealed missing navigation, selection, and presentation semantics. Therefore the current test set is strong regression evidence for enumerated behavior, but incomplete evidence for the full Far interaction grammar.

### Analytical inferences

- The concentration in `FarWorkspace` increases the probability that a change to one interaction mode will bypass or conflict with another mode. This is the strongest explanation for clusters such as arrows working in one context but not another, selection extension not composing with discontinuous navigation, and semantic resource kinds existing in metadata but not reaching presentation consistently.
- A 50 ms poll interval is simple and safe, but it creates unnecessary wake-ups and couples task responsiveness to a timer. A wakeable event source would be both simpler to reason about and more efficient once terminal, task, signal, and timer events share one reactor.
- Statically including Wasmtime, SFTP, archive, and embedded terminal machinery makes deployment convenient but conflicts with progressive extensibility and pay-for-use resource consumption. Far-like utility favors a compact resident core with extensions loaded only when invoked.
- The current performance threshold is too permissive to drive architectural quality. A path can remain below 16 ms while allocations, binary size, memory retention, idle wake count, or tail latency regress substantially.
- The architecture should be simplified before adding another large tranche of behavior. Continuing to add workflows directly to `FarWorkspace` would make the declared platform boundaries less true even if every new workflow receives tests.

## What “Far-like utilitarian engineering” means

Far Manager's relevant standard is not visual imitation or Windows-specific implementation. It is a practical engineering posture:

### 1. Directness

- One keystroke resolves to one semantic command through a short, inspectable path.
- Current item, explicit selection, source panel, and destination panel have stable meanings.
- Commands do not infer state through UI accidents; availability and denial are computed explicitly.
- The common path does not pass through general machinery intended only for rare extensions.

### 2. Idle means idle

- No animation or task means no render and no periodic wake-up.
- Worker threads are lazy or sleeping without timed polling.
- Terminal hangup, suspend, and external handoff are terminal lifecycle events, not exceptional spin conditions.

### 3. Work scales with what is visible or requested

- Navigation is O(1) or O(log n), not a full collection rebuild.
- Rendering and semantic decoration are O(viewport), with stable cached data outside changed regions.
- Directory metadata is loaded incrementally and only when required by columns, sorting, filtering, highlighting, or commands.
- Search, copy, hashing, archive, and remote work are streamed, bounded, cancellable, and generation-checked.

### 4. Features pay their own cost

- Core panel navigation does not initialize plugin runtimes, SSH, archives, or PTYs.
- Optional capabilities can be compiled separately, loaded lazily, or run out of process.
- The base binary and resident state remain understandable without the extension system.

### 5. Behavioral completeness over decorative breadth

- Navigation, selection, command applicability, and filesystem safety are treated as grammars with cross-product tests.
- A feature is not complete because its happy-path unit test passes; modifier interaction, overlays, focus changes, stale data, and unavailable capabilities are included.
- Visible distinctions—file, folder, link, package, virtual root, selected, current, denied, stale—are semantic contracts.

### 6. Recovery without hidden machinery

- Operations have immutable plans, journals, explicit conflict decisions, and cancellation summaries.
- Configuration changes validate before commit and roll back to the last valid state.
- External tools and embedded terminal sessions restore the terminal deterministically.

Near already embodies much of sections 5 and 6 in its contracts. The work now is to make sections 1 through 4 equally true in the implementation.

## Current architecture map

```text
near-fm
  ├─ composes platform/config/provider/plugin services
  └─ constructs FarWorkspace
       ├─ owns almost all file-manager mutable state
       ├─ resolves terminal input and keymaps
       ├─ dispatches Far commands
       ├─ coordinates providers/search/operations/tasks
       ├─ owns persistence adapters and histories
       ├─ owns overlays/viewer/editor/quick-view/PTY
       └─ renders the entire workspace through near-ui

near-ui
  ├─ reusable surfaces and scene semantics
  ├─ FarWorkspace application coordinator
  ├─ terminal application loops
  └─ direct dependencies on most domain services

near-runtime
  └─ bounded worker pool and small blocking future executor

near-core
  └─ command/resource/provider/capability contracts

near-terminal
  └─ Crossterm lifecycle, normalized input, signal handling

provider/service crates
  ├─ local filesystem
  ├─ archive
  ├─ SFTP
  ├─ search
  ├─ operations
  └─ handlers/plugins/PTY/config
```

This graph is acyclic at the Cargo level, which is good. The problem is logical ownership: `FarWorkspace` and `near-ui` sit at the center of too many reasons to change.

## Target architecture

The target should use fewer runtime concepts, not more. The essential architecture is:

```text
Terminal / signal / timer / task completion
                  │
                  ▼
              Reactor
                  │ Msg
                  ▼
       update(&mut AppModel, Msg)
                  │
          ┌───────┴────────┐
          │                │
       Effects        Invalidation
          │                │
          ▼                ▼
   bounded services     view(model)
          │                │
          └── Msg ─────────┴── semantic scene → terminal diff
```

### State ownership

`AppModel` should be composed from cohesive state domains:

```text
AppModel
  ├─ WorkspaceModel
  │    ├─ PanelModel[2]
  │    ├─ FocusModel
  │    └─ TransferContext
  ├─ InteractionModel
  │    ├─ NavigationState
  │    ├─ SelectionState
  │    ├─ ModifierState
  │    └─ CommandLineState
  ├─ OverlayModel
  ├─ DocumentModel        # viewer/editor/quick view sessions
  ├─ TaskModel            # user-visible task records only
  ├─ HistoryModel
  └─ CapabilityModel
```

Services, stores, providers, and executors belong in an `AppServices`/`EffectContext`, not interleaved with user-visible state. This distinction reduces the present 91-field coordinator and makes snapshots cheap and meaningful.

### Message and effect boundary

All state changes should enter through a finite `Msg` vocabulary:

```rust
enum Msg {
    Input(TerminalEvent),
    Command(CommandInvocation),
    TaskCompleted(TaskId, TaskOutcome),
    ProviderUpdated(PanelId, ListingGeneration, ProviderEvent),
    TimerElapsed(TimerId),
    ExternalReturned(ExternalResult),
    Terminate(Signal),
}
```

`update` must be deterministic and non-blocking. It returns bounded effects such as list, stat, open, plan operation, execute operation, persist state, launch external process, start PTY, schedule timer, or quit. Effects post messages back through the reactor. This makes stale-generation rejection, cancellation, focus changes, and selection composition testable without a terminal or thread timing.

### Reactor

The reactor should block until the earliest real event:

- terminal file descriptor readable or closed;
- self-pipe/eventfd/channel wake from task completion;
- signal notification;
- nearest key-sequence, animation, or retry deadline;
- optional provider watch event.

There should be no fixed 50 ms wake in the steady state. The same reactor contract can use `mio`/`poll` on Unix and the appropriate wait set on Windows. Terminal adapters remain platform-specific; the message/update contract remains portable.

### Rendering

Keep Ratatui private and retain semantic roles. Change the producer side:

- Each state domain increments a small revision when visible state changes.
- View models cache sorted/filtered indices, decorated labels, columns, and status text by relevant revisions.
- Panel rendering visits only viewport rows.
- Overlays invalidate only their semantic region, while terminal diffing remains the final output optimization.
- Expensive width calculation, formatting, regex evaluation, and metadata-derived decoration are cached per entry and invalidated by metadata/theme/column revisions.
- A render may still assemble a semantic scene for API simplicity, but its cost must be proportional to visible cells and changed view-model revisions.

### Feature boundaries

Use a compact base composition:

```text
near-fm-core
  core + terminal + semantic UI + local provider + operation contracts

optional capability hosts
  archive adapter
  SFTP adapter
  embedded PTY adapter
  process plugin host
  Wasm plugin host
```

The Wasm engine should initialize only when a Wasm extension is loaded. A stronger low-resource option is a separate `near-plugin-host` process, preserving crash isolation and allowing the core binary to omit Cranelift. Archive and SFTP providers should depend only on provider/stream/operation contracts, not `LocalFileProvider` concrete helpers.

### Public API

`near-app` should become a curated application vocabulary rather than a broad re-export layer:

- stable: `Application`, `Surface`, `Command`, `ResourceProvider`, `Task`, `Theme`, `Keymap`, dialogs, menus, collection model;
- advanced modules: configuration, search, operations, macros, terminal capability contracts;
- private: concrete runtime pool, internal scene renderer, Far coordinator, provider implementations, Crossterm/Ratatui details.

Public capability traits should be narrow and object-safe where runtime composition is needed. Data contracts crossing process/plugin boundaries should be versioned separately from Rust convenience APIs.

## Proposed resource envelope

These are target budgets requiring baseline calibration on the reference macOS and Linux machines. They are architectural controls, not claims about current performance.

| Dimension | Base target | Extended target | Measurement |
|---|---:|---:|---|
| Idle CPU | ≤0.2% of one core | ≤0.5% with provider watches | 30-second PTY sample, no tasks |
| Idle wakeups | 0 periodic wakes | only declared watcher deadlines | OS wake/event trace |
| Idle threads | 1 UI thread, lazy workers | capability-specific sleeping threads | process thread count |
| Warm key-to-model p95 | ≤2 ms | ≤4 ms under active listing | deterministic model benchmark |
| Key-to-present p95 | ≤8 ms | ≤16 ms through tmux/SSH matrix | timestamped PTY probe |
| Base release binary | ≤8 MiB stripped | capability bundles measured separately | artifact size gate |
| Base idle RSS | ≤25 MiB | each capability declares delta | clean-start resident set |
| Startup to first frame | ≤75 ms warm | ≤150 ms cold | PTY first-frame timestamp |
| 100k-entry navigation | no collection-size-dependent key latency | p95 ≤4 ms | synthetic provider |
| Render allocations | steady-state bounded by viewport | zero growth over 10k keys | allocator instrumentation |
| Task completion wake | ≤2 ms from completion to update | ≤8 ms under load | reactor benchmark |

If measurements show these exact values are not portable, change them with recorded data. Do not remove the dimensions.

## Behavioral completeness model

The recent discoveries demonstrate that individual key tests are insufficient. Qualification needs a generated cross-product over five dimensions:

1. **Navigation command:** up/down/left/right/home/end/page up/page down/mouse row/search jump.
2. **Selection mode:** none, contiguous Shift extension, explicit toggle, saved selection, filtered view, non-selectable item.
3. **surface context:** panel, menu, dialog list, history, settings, search results, viewer/editor lists.
4. **modifier lifecycle:** press, repeat, release, focus loss, legacy terminal without release events.
5. **data shape:** empty, one row, partial page, multiple pages, filtered gaps, stale generation, disappearing resource.

For every combination declared applicable, assert:

- current item;
- selected set and anchor;
- viewport start;
- semantic row roles and kind markers;
- available commands and denial reasons;
- task/provider side effects;
- exact behavior after the next unmodified navigation command.

Far compatibility cases must be sourced from operator observations or an executable reference capture, then encoded independently of Near's current implementation. This prevents a regression test from merely preserving the behavior that happened to be written first.

## Assessment of the original migration program

The document originally listed `A0` through `A7`: one baseline stage plus seven migration stages. That is a reasonable architecture program, but only part of it directly produces Far-compatible behavior.

| Original stage | Movement toward Far | Assessment |
|---|---|---|
| A0 — Baseline and guardrails | Indirect | Necessary for Far-like resource discipline, but it measures Near rather than defining the behavior Near must preserve. It needs a Far behavior baseline at the same time. |
| A1 — Deterministic application kernel | Strong | The best structural move. Stable current item, selection, focus, source, destination, and command transitions become explicit. It must be designed from captured Far behavior, not Near's current behavior. |
| A2 — Split the coordinator | Indirect | Reduces escaped defects, but “controllers” could merely replace one god object with several mutable mini-coordinators. Prefer pure state domains and reducers with one update owner. |
| A3 — Wakeable runtime | Strong engineering alignment | Directly advances Far's low-idle-cost and immediate-response character. The target must optimize latency and energy together; minimizing thread count is not valuable if it adds first-operation delay. |
| A4 — Pay-for-use capabilities | Moderate | Matches Far's compact core and progressive plugin model. Compile-time feature fragmentation or a heavyweight process boundary could make archives, remote panels, and plugins feel less integrated, which would move away from Far. Lazy seamless activation is the requirement. |
| A5 — View-model and render economy | Strong at scale | Supports fast navigation in large panels. Region invalidation is not automatically necessary because terminal backends already diff cells; caching and O(viewport) derivation should be proven first. |
| A6 — External interaction oracle | Essential but too late | This should begin before A1 and continue through every stage. Otherwise the migration can make an elegant model of incorrect navigation, selection, menus, or presentation. |
| A7 — Public API reduction | Weak direct Far value | Valuable platform hygiene, but it does not make `near-fm` more Far-like by itself. It belongs last, after the application kernel and behavior contracts stabilize. |

### What the original program gets right

- It recognizes that low resource use is architectural: event-driven waiting, bounded work, viewport scaling, and lazy capabilities.
- It addresses the root cause behind cross-mode interaction omissions rather than adding more key-specific patches.
- It preserves the semantic command/resource model, operation safety, and portability improvements that should remain better than Far's platform-specific internals.
- It avoids a rewrite and retains an incremental facade, which is important for protecting existing workflows.

### What was missing or not right

#### 1. Far behavior was treated as a late test source

The oracle must precede kernel design. `FAR-PANEL-001`, `FAR-PANEL-007`, `FAR-SELECT-001`, and `FAR-MENU-002` are still partial in `project/far-parity.toml`; these are precisely the interaction semantics most likely to be frozen incorrectly if Near's existing model becomes the new kernel without an independent reference.

#### 2. There was no explicit operator-visible parity closure stage

The program could complete all architectural stages while nine parity items remained partial: dual panels, filename lookup, direct multi-selection, modifier keybars, settings, terminal compatibility, shell profiles, viewer policy, and editor policy. Architecture enables parity but does not deliver it automatically.

#### 3. Far's presentation grammar was underrepresented

Far's utility comes from dense visible state as well as commands: resource kinds, current versus selected item, source/destination relationship, sort/filter state, panel mode, command availability, keybar layers, status text, and denial reasons. The original plan concentrated on model and render cost without a dedicated semantic-presentation contract.

#### 4. Command honesty was not a migration invariant

The recent menu audit showed that an enabled action can still be inert or inapplicable. Every stage needs a rule that visible commands derive applicability, explanation, and denial from the same state used by execution. This is as important to Far-like certainty as key bindings.

#### 5. Migration equivalence was unspecified

An incremental facade alone does not prove unchanged behavior. During extraction, old and new reducers should run in shadow for captured workflows and compare current item, selection, viewport, overlay, command availability, effects, and semantic output before ownership switches.

#### 6. Persistence and session durability lacked explicit protection

Histories, shortcuts, viewer/editor positions, settings, panel locations, and operation journals are part of the daily Far experience. State extraction needs schema compatibility, migration, crash-recovery, and restart tests rather than treating stores merely as handles moved into an effect context.

#### 7. Capability modularity could harm seamless workflows

Far users do not care whether archive and remote panels are plugins; they care that entering them is immediate and uniform. The correct goal is zero idle cost before activation with no visible installation/configuration ceremony after the capability is available. Compile-time feature combinations should not become the primary product model.

#### 8. Some proposed budgets were hypotheses

The dimensions are correct, but exact targets such as 8 MiB binary size or 25 MiB RSS need baseline data from macOS and Linux before becoming release blockers. Relative ratchets can begin immediately; absolute limits should follow measured decomposition by capability.

## Revised migration program

### A0 — Far contract and resource baseline

- Capture build-matched Far behavior before changing ownership: panel edges, pages, Shift ranges, discontinuous selection, lookup, keybar layers, menus, command line, screen switching, viewer/editor handoff, and restart persistence.
- Give every case an external observation ID, terminal assumptions, expected semantic state, and allowed Near-native deviation.
- Measure startup, first frame, RSS, threads, wakeups, key-to-present latency, binary composition, allocation rate, large-directory behavior, and capability activation deltas.
- Start relative regression ratchets immediately; calibrate absolute budgets from the decomposition.
- Exit criterion: all migration-critical behavior and resource dimensions have independent macOS evidence, with Linux evidence for portable contracts.

### A1 — Interaction kernel and semantic presentation

- Introduce `AppModel`, `AppServices`, `Msg`, `Effect`, and deterministic `update` internally.
- Move current item, selection anchor/set, viewport, panel focus, peer context, modifier lifecycle, lookup, and command applicability first.
- Define semantic row and chrome state for resource kind, current, selected, source/destination, filter/sort, keybar layer, and denial.
- Run old and new transitions in shadow against the A0 corpus until state, effects, and semantic output agree.
- Exit criterion: `FAR-PANEL-001`, `FAR-PANEL-007`, `FAR-SELECT-001`, and `FAR-MENU-002` have operator-backed workflows through the new kernel.

### A2 — Strangler extraction of state domains

- Extract panel/listing, overlay, document, task, history, macro/extension, and external-session state as data plus pure reducers.
- Keep one update owner; do not create independently mutable controller objects.
- Move providers, stores, clocks, executors, and platform services into the effect context.
- Ratchet `FarWorkspace` fields, methods, and lines downward; forbid new domain behavior in the facade.
- Exit criterion: the facade only composes domains, translates compatibility calls, and invokes update/view.

### A3 — Event-driven runtime and lifecycle

- Integrate terminal readiness/hangup, signals, task completion wakes, provider watches, and exact key/timer deadlines in one reactor contract.
- Keep workers sleeping or lazy according to measured first-use latency; do not optimize thread count in isolation.
- Preserve suspend/resume, console screen switching, nested TUI, panic restoration, and stale-generation rejection.
- Exercise keyboard repeat and completion-to-present latency through real PTYs, tmux, and the terminal matrix.
- Exit criterion: no periodic idle wake, no leaked process after terminal loss, and both idle and interaction latency budgets hold.

### A4 — Data path and render economy

- Cache sorted/filtered indices, decorations, widths, columns, formatted rows, and provider metadata by explicit revisions.
- Keep navigation and selection independent of total collection size; keep rendering proportional to the viewport.
- Use terminal cell diffing as the default; add region invalidation only where measurements prove scene construction remains material.
- Test empty, one-item, partial-page, 1k, 100k, remote, archive, stale, and rapidly changing panels.
- Exit criterion: stable key latency, bounded allocations, no steady-state growth, and correct dense semantic presentation at every scale.

### A5 — Seamless pay-for-use capabilities

- Separate process and Wasm plugin activation so Wasmtime/Cranelift initialize only when a Wasm extension is actually used; evaluate a helper process against in-process lazy loading with measured latency and memory.
- Make archive, SFTP, embedded PTY, and other providers discoverable and uniform without imposing idle initialization cost.
- Replace production dependencies on `LocalFileProvider` concrete helpers with provider-neutral location, stream, staging, and operation contracts.
- Preserve one navigation and operation grammar across local, archive, remote, generated, device, and plugin panels.
- Exit criterion: base resource budgets improve without adding visible ceremony or provider-specific interaction forks.

### A6 — Daily Far workflow closure and durability

- Close the remaining settings, terminal, shell, viewer, and editor policy items: `FAR-CUSTOM-004`, `FAR-PLAT-001`, `FAR-SHELL-006`, `FAR-VIEW-005`, and `FAR-EDIT-005`.
- Verify histories, shortcuts, panel locations/modes, selections where applicable, viewer/editor positions, configuration, and journals across restart, invalid configuration, crash, and external change.
- Assert command applicability and denial consistency across menus, keybars, palette, mouse, and direct shortcuts.
- Run representative operator loops: navigate → select → compare → view/edit → copy/move/delete → shell → return → restart.
- Exit criterion: all current Far parity items are operator-evidenced, with every intentional Near-native deviation documented.

### A7 — Public harvest and release ratchet

- Audit `near-app` only after the kernel vocabulary is stable; expose the minimal contracts proven by non-file-manager consumers.
- Keep Far policy, dual-panel assumptions, concrete providers, runtime internals, Ratatui, and Crossterm outside the application-facing API.
- Make the A0 behavior corpus, resource envelope, artifact composition, platform matrix, and durability loops mandatory release evidence.
- Verify macOS and Linux clean checkouts plus Windows build/test/adapter contracts.
- Exit criterion: the public platform remains independently useful while `near-fm` meets the Far behavior and efficiency contracts.

## How the revised program takes Near closer to Far

It does so in three distinct ways:

1. **Behavioral fidelity:** A0, A1, and A6 define and close the interaction grammar users actually experience—panel movement, selection, command certainty, dense presentation, terminal continuity, and durable daily state.
2. **Engineering fidelity:** A2, A3, and A4 produce the direct, bounded, event-driven implementation behind Far's responsiveness and low resource use.
3. **Extensibility fidelity:** A5 preserves a compact core with seamless provider/plugin activation, while A7 prevents the reusable platform from leaking file-manager assumptions.

The important correction is that architecture is no longer treated as a proxy for parity. Far behavior is captured first, preserved during extraction, and closed explicitly after the structural work.

## Decision summary

### Keep

- Rust and the current safety posture.
- Semantic commands, resources, roles, and backend isolation.
- Provider contracts and generation-aware asynchronous work.
- Immutable operation plans and explicit presentation/denial contracts.
- Ratatui and Crossterm as private implementation substrates.
- Model, semantic render, PTY, qualification, and external-consumer evidence layers.

### Change

- Replace `FarWorkspace` as the effective architecture with a thin facade over a deterministic kernel.
- Replace periodic polling with a unified wakeable reactor.
- Separate mutable state from services and persistence handles.
- Make expensive capabilities lazy and separately accountable.
- Broaden performance from one latency test to a resource envelope.
- Make operator discovery a first-class, independently sourced conformance corpus.

### Avoid

- A wholesale rewrite.
- Splitting every module into a crate before ownership is stable.
- Introducing a general async runtime merely to replace a small task pool.
- Retaining a 50 ms tick because it is operationally convenient.
- Treating a green qualification manifest as proof of unspecified interaction behavior.
- Adding new Far workflows directly to the old coordinator without first assigning state and effect ownership.

## What would falsify this assessment

This assessment should be revised if measurements show that:

- `FarWorkspace` changes are consistently isolated despite its breadth and cross-mode defects do not correlate with coordinator edits;
- the fixed 50 ms wake has no measurable energy, responsiveness, or complexity cost on supported platforms;
- statically linked capability hosts produce negligible binary/RSS/startup cost after stripping and lazy initialization;
- the current interaction corpus detects the documented operator misses before implementation changes; or
- extracting a deterministic kernel demonstrably increases state duplication, latency, and defect rate without reducing coordinator coupling.

No current evidence establishes those conditions.
