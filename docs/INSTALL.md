# Installing Near

Near is pre-release software. Release binaries are the simplest way to install `near-fm`; building from source is the fallback and the development path.

## Release archive

1. Open the [latest GitHub release](https://github.com/juanandresgs/NearManager/releases/latest).
2. Download the archive for your platform, its `.sha256` sidecar, the matching provenance JSON, and `SHA256SUMS`.
3. Verify the downloaded files before extracting:

   ```sh
   sha256sum -c SHA256SUMS --ignore-missing
   python3 tools/package_release.py verify \
     --archive near-PLATFORM.tar.gz \
     --provenance PLATFORM.provenance.json \
     --checksum near-PLATFORM.tar.gz.sha256
   ```

   On macOS, `shasum -a 256 -c` can verify an individual `.sha256` file. On Windows, use `Get-FileHash -Algorithm SHA256` and compare it with the sidecar.

   If the GitHub CLI is installed, also verify the release attestation:

   ```sh
   gh attestation verify near-PLATFORM.tar.gz \
     -R juanandresgs/NearManager \
     --predicate-type https://spdx.dev/Document/v2.3 \
     --signer-workflow juanandresgs/NearManager/.github/workflows/release.yml
   ```

4. Extract the archive and place `near-fm` and the companion binaries in a directory on `PATH`, such as `~/.local/bin`. Windows binaries use the `.exe` suffix.
5. Run `near-fm --version`, then start `near-fm` from a terminal.

Release archives include `near-fm`, `near-view`, `near-proc`, and `near-demo`. They do not modify shell startup files or install system services.

## Build from source

Prerequisites:

- Rust 1.88 or newer, including Cargo.
- Python 3.11 or newer for repository validation and provenance verification.
- A native C toolchain and OpenSSL development files. On Debian or Ubuntu, install `build-essential`, `pkg-config`, and `libssl-dev`. On macOS, install Xcode Command Line Tools. On Windows, install Visual Studio Build Tools with the C++ workload.

Then run:

```sh
git clone https://github.com/juanandresgs/NearManager.git
cd NearManager
cargo install --path apps/near-fm --locked
near-fm --version
near-fm
```

To install the companion applications, repeat `cargo install --path` for `apps/near-view`, `apps/near-proc`, and `apps/near-demo`.

Default configuration is embedded in the binaries. User configuration is stored in the platform config/data directory; no checkout-relative `specs/` directory is required at runtime. See `docs/near-configuration.md` for precedence and workspace-trust behavior.

## Verify a source checkout

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
python3 tools/validate_project.py
python3 tools/validate_abstraction_policy.py
```

Full production qualification also requires platform-specific operator evidence and is not implied by a successful local build.
