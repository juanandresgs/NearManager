# Near Performance Budget

The reference interaction benchmark runs on GitHub `macos-14`, renders the `FarWorkspace::demo()` dataset at 100×30 cells, performs 100 warm-up navigation/render iterations, then measures 1,000 `Down` key-to-semantic-snapshot iterations. The sorted p95 must remain below 16 milliseconds.

A second release benchmark constructs a 100,000-item reusable `CollectionSurface`, positions the
cursor in the middle, and measures 1,000 alternating one-row movements. Its p95 must remain below
250 microseconds. This specifically prevents interaction-kernel integration from rebuilding,
filtering, sorting, or scanning the complete collection during viewport-only navigation.

This headless benchmark intentionally measures Near's command, model, layout, and semantic render path without terminal-emulator scheduling noise. Release investigations may add live terminal traces, but they do not replace the deterministic regression gate.

On macOS and Linux, interactive runtimes block in `TerminalEventReactor` until terminal input, terminal hangup, a task-completion wake, a termination signal, or the exact pending key-sequence deadline. Rendering remains invalidation-driven and there is no fixed idle timer. The Windows fallback retains bounded polling until its native wait-set adapter is implemented and is not part of the first production declaration.

Startup does not issue a blocking keyboard-protocol query. Known compatible terminal environments
select enhanced input without a round trip; `NEAR_KEYBOARD_PROTOCOL=enhanced` or `legacy` provides
an explicit operator override.

The scheduled workflow runs weekly and can be dispatched manually. A regression requires either restoring the budget or changing `REQ-PERF-001` with measured rationale and architecture review.
