# M5 Portability and Delivery Evidence

Date: 2026-06-23

## Portability

- `near-local-fs` uses platform adapters for reversible path encoding, metadata, symlinks, permissions, device boundaries, Trash/Recycle Bin, and typed platform extensions.
- Linux uses XDG configuration/data paths and Freedesktop Trash metadata. macOS-only metadata is absent without becoming a provider failure.
- Windows drive and UNC paths use reversible UTF-16LE locations; file attributes, reparse state, ACLs, and alternate streams are typed extensions.
- `near-terminal` registers the termination signals available on each platform. `near-pty` selects native shells and includes a Windows ConPTY smoke test.
- `near-fm` selects platform configuration/data roots and structured handler documents without changing shared command, keymap, theme, provider, or surface APIs.
- CI run `28043599502` passes the full workspace test and clippy suites natively on macOS, Linux, and Windows: <https://github.com/juanandresgs/NearManager/actions/runs/28043599502>. A local Windows cross-target workspace check also passes.
- A native Linux/aarch64 Docker run using Rust 1.94.1 passes `cargo test --workspace --locked` and `cargo clippy --workspace --all-targets --locked -- -D warnings`. The run uses the platform shell contract without installing zsh.
- The complete Windows x86-64 test set links with MinGW. Wine 9 executes the shared applications and Windows-specific root, UTF-16 drive/UNC, file-attribute, reparse, ACL, alternate-stream, provider, CLI, and navigation tests successfully. Wine's missing ConPTY output and Windows PowerShell are documented compatibility-layer limitations, not replacements for native CI.

## Releases and Migrations

- Tagged releases require a cryptographically signed source tag, verification against `project/security/release-signers`, and matching versioned migration/release notes.
- `docs/releases/v0.1.0.md` supplies the current version's platform, migration, compatibility, limitation, exception, and rollback notes.
- `tools/package_release.py` creates deterministic archives plus checksum sidecars and provenance binding each archive to `GITHUB_SHA`, a source-tree digest, `Cargo.lock`, toolchain versions, and exact members. Two local macOS/aarch64 rehearsals produced the identical SHA-256 `191535d99c3719cb316c35069e6a7e61a48fd07b4a74788fe769234d1620009d`.
- Each matrix build scans its exact archive with Anchore Syft; the publish job checksums the archives, per-platform SBOMs, provenance records, and audit outputs together.
- A local Syft scan of the deterministic macOS archive produces SPDX 2.3 with the archive as document subject and all four distributed binaries as files.
- Locked multi-platform binaries, dependency audit JSON, license inventory, SPDX JSON SBOM, SHA-256 checksums, and portable build-provenance records are generated before publication. The private `v0.1.0` release could not use GitHub-hosted attestations; the public release workflow now attests each archive and its SBOM.
- `CHANGELOG.md`, `docs/releases/README.md`, and the schema/WIT/process compatibility gates define migration responsibilities.
- Vulnerability exceptions require an owner, compensating controls, and expiry; none are currently accepted.
- Private release `v0.1.0` was published on 2026-06-23 from the verified signed tag: <https://github.com/juanandresgs/NearManager/releases/tag/v0.1.0>. Release workflow run `28048075041` completed successfully: <https://github.com/juanandresgs/NearManager/actions/runs/28048075041>.
- A clean consumer download verified every entry in `SHA256SUMS` and passed `tools/package_release.py verify` for the macOS, Linux, and Windows archives against their published provenance and checksum sidecars.

## Governance

- CI validates project records, Rust public APIs, WIT, process protocol, and release policy.
- `CODEOWNERS` and the pull-request template require project, release, and security review for their normative areas.
- Verified requirements link to checked-in evidence and the validator rejects broken identifiers, references, uncovered capabilities, and missing evidence.

## Verification Scope

`REQ-PORT-001`, `REQ-PORT-002`, `REQ-REL-001`, and `REQ-GOV-001` are verified. The native CI matrix, signed private release, audits, checksums, SBOMs, provenance records, and consumer-side verification provide the required external delivery evidence.
