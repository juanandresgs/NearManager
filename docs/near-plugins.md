# Near Extension Host

`near-plugins` hosts third-party code through two isolated runtimes. WebAssembly components use Wasmtime 38, the newest release compatible with Near's Rust 1.88 MSRV, and are the preferred portable tier. macOS process extensions provide a narrow JSON command adapter for tools that cannot target the Component Model. Trusted first-party Rust remains compile-time only; native dynamic libraries are not a public ABI.

## Package Structure

Each package directory contains `plugin.toml` and a relative component artifact, defaulting to `component.wasm`. Manifests have schema and plugin versions, a compatible `near:plugin` interface requirement, declared commands, optional command-prefix mappings, providers, versioned capabilities, and memory/fuel/wall-clock limits. Absolute and parent-traversing artifact paths are rejected.

Process packages set `runtime = "process"`, name one contained executable, declare commands, and negotiate the versioned protocol in `specs/process-plugin-protocol.md`. Protocol 0.1.0 deliberately supports no direct capabilities or providers. It uses fresh one-shot processes, bounded JSON request/response documents, semantic effects, and explicit time and byte limits. Its immutable baseline is checked in CI.

`near-fm` discovers installed packages from `~/Library/Application Support/near/plugins`, workspace packages from `.near/plugins`, or an explicit `NEAR_PLUGIN_DIR`. Workspace packages require an explicit trust entry. `NEAR_PLUGIN_GRANTS` can select a different grants document.

## Authority

The host exposes only three versioned calls: structured logging, notifications, and bounded resource reads. A call succeeds only when the capability is both declared by the manifest and granted by the user. Grants are inspectable and revocable in `plugin-grants.toml`; workspace trust is separate from capabilities.

Local file reads are mediated by Near's resource identity and capped to 1 MiB per call. Components cannot open paths directly because no WASI filesystem is linked.

Process extensions receive descriptive command context but no host authority. Near clears their environment and, on macOS, uses a deny-default Seatbelt profile that permits package-private reads, the package executable, and standard output/error only. Tests prove denial of external file reads, inherited environment, loopback connections, and arbitrary child execution. Brokered process capabilities require a future protocol revision rather than sandbox widening.

## Isolation and Limits

Every invocation receives a fresh Wasmtime store. Linear memory, instances, memories, and tables are limited. Fuel bounds deterministic computation and epoch interruption bounds wall-clock execution. A trap or exhausted limit returns a structured extension failure and does not terminate Near. Trusted first-party native crates remain compile-time integrations rather than a dynamic ABI.

Every process command receives a fresh child. Request size, output size, and wall-clock time are bounded; crashes and nonzero exits are isolated. The macOS launcher currently uses deprecated `/usr/bin/sandbox-exec`, so its Seatbelt implementation is replaceable and must be reassessed before stable distribution. The JSON protocol does not depend on Seatbelt syntax.

## Application Integration

`near-core::CommandExtension` is host-neutral. Extension descriptors enter the normal command registry, command palette, help/macro availability path, and dispatcher. Extensions may additionally contribute F11 menu actions, editable string settings, command-line prefixes, and authored help topics. Every menu action still targets a registered semantic command, setting submissions receive strict generated argument schemas, and unavailable commands remain visible with their denial reason instead of silently disappearing. Optional `[[prefixes]]` manifest entries map `prefix:arguments` input to a declared command and string argument without bypassing that path. Effects are semantic messages, navigation, resource opening, or task identifiers. `F11` and `near.extensions.show` expose the actionable extension menu in the Far workspace; extensions without explicit menu metadata fall back to their registered commands.

The component `provider` export adapts paged resource items into `ResourceProvider`, including provider-neutral locations, metadata, continuation, cancellation checks, and capability metadata. Version 0.1 does not yet expose provider `stat` or `open`; those operations return explicit unsupported errors rather than ambient fallbacks.

## WIT Evolution

`specs/plugin.wit` uses the full package version `near:plugin@0.1.0`, `@since` annotations for stable APIs, and `@unstable` feature gates for experimental APIs. `specs/plugin-v0.1.0.wit` is immutable. `tools/check_wit.py` rejects interface changes without a package-version change, while `near-plugins` parses and validates the current package through `wit-parser`.
