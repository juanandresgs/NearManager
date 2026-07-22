# Release and Migration Policy

Every stable tag `vX.Y.Z` has a reviewed `docs/releases/vX.Y.Z.md` file. The document lists supported platforms, user-visible changes, configuration and plugin migrations, compatibility breaks, known limitations, vulnerability exceptions, and rollback guidance. The release workflow refuses tags without that file.

Release tags must be cryptographically signed. GitHub Actions builds locked multi-platform binaries, publishes dependency and license audit results, creates an SPDX JSON SBOM from each exact distributed archive, generates SHA-256 checksums and deterministic build-provenance records, creates GitHub artifact and SBOM attestations, and attaches all outputs to the GitHub release.

Configuration schema changes require a registered migration and fixture in `near-config`. WIT and process-protocol changes require a new version plus immutable compatibility baseline. Rust API changes follow `docs/near-api-compatibility.md`.

Consumers verify archives with `sha256sum -c SHA256SUMS`, `python3 tools/package_release.py verify --archive ARCHIVE --provenance PROVENANCE --checksum CHECKSUM`, and optionally `gh attestation verify ARCHIVE -R juanandresgs/NearManager` from the signed source tag.

Each platform build also runs `tools/package_release.py`, which creates a deterministic archive, a
checksum sidecar, license texts, the top-level README, and a version-2 JSON provenance record binding the archive to the reviewed
`GITHUB_SHA`, clean-source state, raw Git filename/content digest, `Cargo.lock`, toolchain versions,
exact archive members, and extracted-binary help/version smoke tests. Verification reruns those
smoke tests from the archive and rejects dirty-source provenance by default. Anchore Syft scans that
exact archive. The same packaging tool supports explicit dirty local rehearsals with
`--allow-dirty`; such artifacts cannot pass normal release verification. Portable provenance and
checksums remain usable without a GitHub account; GitHub attestations add an online origin check.
