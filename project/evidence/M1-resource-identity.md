# M1 Provider-Neutral Resource Identity Evidence

Date: 2026-06-23

## Implemented Slice

- `near-core` defines `ResourceIdentity` as provider-scoped durable identity and exposes `ResourceMetadata::identity_for`.
- `near-local-fs` supplies device/inode stable identity independently of display path bytes.
- `near-reference-providers` supplies process, search-result, and plugin-catalog providers with paged cancellation-aware listings and explicit capability sets.
- Search collections emit original source `ResourceRef`s, allowing normal commands to resolve through the source provider rather than a display-only result wrapper.
- Process identities use PID scope independently of command display text.
- Plugin identities use plugin IDs independently of names, descriptions, and versions.
- `near-demo` renders process and plugin domains as focused/peer generic collection surfaces.

## Automated Evidence

- The core identity test proves two different display locations map to one provider-scoped stable identity.
- The cross-provider identity test creates a real local file, renames it, and proves device/inode identity survives while the location changes.
- The same test proves search results preserve the exact renamed local `ResourceRef` and stable identity.
- Process and plugin fixtures change display metadata while retaining equal resource references and stable identities.
- Capability tests prove process and plugin resources omit unsupported write, Trash, and delete operations; search results advertise no surrogate item capabilities.
- A shared workflow test executes the same collection movement command over process, plugin, and search-result surfaces.
- `near-demo` snapshot and focus tests prove a mixed non-filesystem workspace uses the common surface and peer contracts.

## Requirement Status

- `REQ-RES-001` is verified. Local files, search results, processes, and plugin items share `ResourceRef`; durable identity is independent of display paths and names; unsupported capabilities are absent.
