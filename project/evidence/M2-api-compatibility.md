# M2 Rust API Compatibility Evidence

Date: 2026-06-23

## Policy

- `docs/near-api-compatibility.md` defines crate stability levels, pre-1.0 and post-1.0 SemVer commitments, MSRV changes, feature semantics, deprecation minimums, and evolution rules.
- The workspace declares Rust 1.88 and every published crate inherits `workspace.package.rust-version`.
- Facade types use private fields and constructors; extensible public enums use `#[non_exhaustive]` where external matching is expected; provider-specific metadata uses typed extension maps.

## Automated Enforcement

- `tools/check_public_api.py` rejects Ratatui, Crossterm, Tokio, `PathBuf`, task-handle, near-fm, direct backend-dependency, provider-signature, single-surface, and MSRV inheritance violations.
- `.github/workflows/ci.yml` runs formatting, strict Clippy, the full workspace suite, project-definition validation, and the public API audit on pushes and pull requests.
- `.github/workflows/release-api.yml` establishes the first private release baseline and runs the upstream `cargo-semver-checks` v2 action against the previous release tag before later publication completes.

## Requirement Status

- `REQ-API-002` is verified by the documented policy, inherited MSRV, structural audit, and release-tag SemVer compatibility gate.
