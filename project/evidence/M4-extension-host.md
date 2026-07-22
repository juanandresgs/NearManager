# M4 Extension Host Evidence

Date: 2026-06-23

## Implemented Slice

- `near-plugins` loads versioned WebAssembly components without WASI ambient authority.
- Manifests declare compatible WIT versions, commands, providers, capabilities, artifact paths, and execution limits.
- User grants and workspace trust are separate, inspectable, and revocable.
- Wasmtime stores enforce memory/object limits, fuel, and epoch deadlines.
- `near-core::CommandExtension` routes extension descriptors and effects through the same registry and workspace dispatcher as built-in commands.
- `near-fm` discovers installed and workspace packages, mediates bounded local resource reads, and exposes an extension catalog.
- Component provider listings adapt into the universal `ResourceProvider` paging model; unsupported v0.1 `stat` and `open` calls fail explicitly.
- Process packages negotiate a SemVer JSON protocol, expose command descriptors through the same `CommandExtension` abstraction, and are selected by `near-fm` discovery without affecting Wasm packages.
- The macOS process launcher clears the environment and applies a deny-default Seatbelt profile with package-only reads, exact executable permission, and standard-stream writes.
- Process invocations enforce request, output, and wall-clock limits and isolate crashes and nonzero exits.

## Automated Evidence

- WIT parsing verifies the full package SemVer and required world/interfaces.
- Compatibility CI compares `plugin.wit` to the immutable baseline for its declared version.
- Process-protocol CI compares `process-plugin-protocol.md` to the immutable 0.1.0 baseline.
- Tests prove workspace trust denial, grant revocation, guest trap isolation, subsequent healthy execution, fuel exhaustion, and memory-limit rejection.
- A checked-in Component Model fixture lowers `log`, `notify`, and `read` through canonical memory/reallocation, proving undeclared and ungranted denials plus granted events and brokered resource reads across the actual guest boundary.
- Compatibility tests reject incompatible manifest ranges, future host-interface imports, and future process-protocol requirements.
- Workspace tests prove extension commands appear in the registry/palette and dispatch through semantic effects with diagnostics.
- Provider adapter tests prove component resource records become generation-preserving universal list pages.
- Existing external handoff tests remain green with the extension host enabled.
- A real macOS sandbox suite compiles hostile C fixtures and proves denial of an external file, inherited `HOME`, a loopback connection, and `/bin/true` execution.
- The same suite proves crash recovery, deadline termination, and output-ceiling enforcement; a Far discovery test proves `runtime = "process"` selects the process host.

## Follow-up Hardening

- Add package-size and compilation-concurrency limits before stable application distribution.
- Replace or revalidate the deprecated macOS Seatbelt launcher before a platform release that removes it.

`REQ-PLUGIN-001`, `REQ-PLUGIN-002`, `REQ-PLUGIN-003`, and `REQ-SEC-002` are verified. Evidence covers both isolation tiers, every published Wasm host import, version compatibility, Far integration, revocation/trust behavior, and the reviewed plugin trust boundary.
