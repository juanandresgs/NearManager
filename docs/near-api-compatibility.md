# Near Rust API Compatibility Policy

## Stability Levels

Near is currently pre-1.0. Published library crates use these explicit levels:

| Crate | Level | Commitment |
|---|---|---|
| `near-core` | preview | Provider, resource, command, and identifier contracts evolve under Cargo SemVer. |
| `near-runtime` | preview | Bounded task and executor contracts evolve under Cargo SemVer. |
| `near-terminal` | adapter | Public normalized events and diagnostics are portable; terminal implementation details remain private. |
| `near-ui` | preview | Backend-independent surfaces, scenes, keymaps, themes, and snapshots are public; Ratatui remains private. |
| `near-app` | application facade | Preferred public dependency for applications; backend and file-manager internals are excluded. |
| `near-search` | preview | Versioned predicates, provider-neutral traversal, streaming events, and result-collection contracts evolve under Cargo SemVer. |
| `near-handlers` | preview | Typed handler documents, argument templates, shell opt-in, and diagnostic contracts evolve under Cargo SemVer. |
| `near-config` | preview | Layer precedence, provenance, migration, trust, and atomic reload contracts evolve under Cargo SemVer. |
| `near-macros` | preview | Semantic recording, context conditions, replay policy, and host validation contracts evolve under Cargo SemVer. |
| `near-pty` | adapter | Native PTY lifecycle, VT snapshots, resize, input, and OSC 7 contracts evolve under Cargo SemVer. |
| `near-plugins` | experimental host | Versioned manifests, grants, Component Model loading, limits, and WIT adapters evolve under Cargo SemVer. |
| `near-reference-providers` | example/reference | Provider contracts are supported; example domain contents are not compatibility promises. |
| `near-archive` | provider/operations | Archive mount, container capability, and operation contracts follow shared provider and plan compatibility rules. |
| `near-sftp` | provider/operations | SFTP profile, transport, provider, reconnect, and transfer-plan contracts follow shared provider and operation compatibility rules. |

Before 1.0, breaking changes require a minor version increment and migration notes. After 1.0, breaking changes require a major version increment.

## Minimum Rust Version

The workspace declares Rust 1.88 in `workspace.package.rust-version`. Every published crate inherits that value. Raising the MSRV requires a minor release before 1.0, release notes, and CI updates.

## Feature Semantics

`near-ui` exposes the additive `embedded-pty` feature, disabled by default, which adds the native `near-pty` adapter and `EmbeddedTerminalSurface`. `near-fm` enables it explicitly. New features must be additive, documented, and disabled by default when they expand platform dependencies or experimental behavior. Removing or changing the meaning of a stable feature is SemVer-breaking.

## Evolution Rules

- Prefer private fields plus constructors and accessors for facade types.
- Mark externally matched enums `#[non_exhaustive]` when future variants are expected.
- Extend resource metadata through typed extension maps when the field is provider-specific.
- Do not expose Ratatui, Crossterm, Tokio handles, `PathBuf`, or file-manager peer assumptions through `near-app`.
- Provider contracts use `Location`, `ResourceRef`, streams, capabilities, generations, and cancellation.
- Deprecations remain available for at least one minor release before removal and include a replacement path.

## Release Gates

`tools/check_public_api.py` runs in normal CI and rejects backend, local-path, file-manager, MSRV, or facade dependency leaks. `.github/workflows/release-api.yml` establishes the first private release as the API baseline, then runs `cargo-semver-checks` for later tags against the previous release tag. Release review also checks migration notes for every intentional breaking change.

The public API audit is intentionally structural and fast; semantic compatibility remains enforced by `cargo-semver-checks` and review.
