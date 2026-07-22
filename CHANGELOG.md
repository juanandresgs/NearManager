# Changelog

All notable user-visible changes follow Keep a Changelog categories. Stable releases use a matching `docs/releases/vX.Y.Z.md` document for upgrade and migration instructions.

## Unreleased

### Added

- Resumable developer, wave, and production qualification with immutable operator evidence,
  disposable mount safety checks, release artifacts, and private public-API consumer validation.
- Revision-bound macOS/Linux operator session packs with terminal/toolchain inventory, exact fixture
  hashes, explicit filesystem capability probes, and Far-parity workflow checklists.
- Ten-slot Far-compatible Temporary Panels with provider-qualified references, safe and full-panel
  modes, UTF-8 import/export, command-output ingestion, labeled menus, stale-resource retention,
  and source reveal without mutating the referenced resources.
- Typed settings now hide advanced entries until requested, retain hidden searchability, and expose
  a real restart-scoped initial-panel policy.
- Multiple retained terminal tabs can occupy either pane, participate in F12 switching, and use
  `Ctrl+O` for reversible zoom without losing PTY state.
- Public installation, contribution, security, dual-license, architecture-specific release, SBOM,
  provenance, and artifact-attestation paths.

### Changed

- Panel command entry and `Ctrl+O` now share one persistent interactive PTY, so shell output, working
  directory, nested applications, and REPL state remain terminal state instead of viewer content.
- The F9 hierarchy, panel navigation, paging, direct and non-contiguous selection, key-repeat
  handling, resource-kind markers, and default file-opening behavior follow the documented Far
  interaction contracts.
- Temporary Panels persist provider-qualified references across restart, appear in side-targeted
  Alt+F1/Alt+F2 location selection with numbered previews, and clear without deleting sources.

### Fixed

- Mounted filesystem roots are denied before operation planning and direct operators to the
  capability-gated device workflow instead of silently doing nothing.
- Settings, menus, and the command palette accept ordinary text input, retain explicit Back paths,
  reject inert actions, and provide a keymap-independent `Ctrl+Alt+Q` emergency exit.
- Release archive smoke testing is safe on supported Python versions, and macOS/Linux wave
  qualification plus Windows cross-target build and Clippy checks are clean.

## 0.1.0 - 2026-06-23

### Added

- Cross-platform filesystem, terminal, configuration, and release adapters.
- Capability-controlled Wasm and isolated process extension contracts.

### Security

- Use the patched Wasmtime 36.0 maintenance line for component isolation while preserving the Rust 1.88 MSRV.

### Migration

- No stable-release migration is required yet. Configuration migrations remain schema-driven and preserve origin diagnostics.
