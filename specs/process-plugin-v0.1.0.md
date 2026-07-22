# Near Process Extension Protocol 0.1.0

Status: stable

Near process extensions are command adapters for tools that cannot target the WebAssembly Component Model. They are a compatibility tier, not a source of ambient operating-system authority. Wasm remains the preferred third-party runtime.

## Package

A package is a directory containing `plugin.toml` and one executable artifact. The executable path must be relative, cannot contain parent traversal, and must canonicalize inside the package.

```toml
schema = 1
id = "example.formatter"
name = "Example Formatter"
version = "1.0.0"
protocol = "^0.1"
runtime = "process"
executable = "bin/formatter"
arguments = ["--near-protocol"]
capabilities = []

[limits]
timeout_ms = 1000
max_request_bytes = 1048576
max_output_bytes = 1048576

[[commands]]
id = "example.formatter.run"
title = "Format Resource"
description = "Formats the focused resource through an external adapter"
category = ["Format"]
safety = "confirmable"
```

Schema 1 rejects unknown fields, unknown safety classes, duplicate command identifiers, incompatible protocol requirements, zero limits, non-process runtimes, and every direct capability declaration. Workspace packages additionally require an explicit trust entry.

## Invocation

Near starts a fresh process for each invocation, clears the environment, sets only `NEAR_PROCESS_PROTOCOL=0.1.0`, writes one UTF-8 JSON request followed by a newline to standard input, and closes standard input.

```json
{
  "schema": 1,
  "kind": "invoke",
  "command": "example.formatter.run",
  "context": {
    "focused_location": "file:///tmp/example.txt",
    "peer_location": null,
    "current": {"provider": "near.local", "uri": "file:///tmp/example.txt"},
    "selected": [],
    "capabilities": ["near.resource.read"]
  },
  "arguments": {}
}
```

The context is descriptive, not authority. In protocol 0.1.0 the child cannot dereference resource URIs or invoke host capabilities. Future process capabilities require a separate brokered protocol revision; they must never be implemented by widening the sandbox to user files, network, or arbitrary child processes.

## Response

The child writes one UTF-8 JSON response to standard output. Standard error is diagnostic-only. Exactly one semantic effect is returned:

```json
{
  "schema": 1,
  "effect": {"kind": "message", "value": "Completed"},
  "diagnostics": []
}
```

Effect variants are:

- `message`: `{ "kind": "message", "value": "..." }`
- `navigate`: `{ "kind": "navigate", "location": "..." }`
- `open`: `{ "kind": "open", "resources": [{"provider": "...", "uri": "..."}] }`
- `task`: `{ "kind": "task", "id": "..." }`

Unknown schemas, malformed JSON, nonzero exits, crashes, timeouts, request overflow, and output overflow are structured extension failures. They do not terminate Near. Process protocol 0.1.0 exports commands only; provider implementations use the Wasm component contract.

## macOS Sandbox

On macOS, Near launches the executable through `/usr/bin/sandbox-exec` with a deny-default Seatbelt profile. The profile imports the platform runtime rules needed by dynamically linked programs, permits execution of only the package executable, permits reads only of that executable and its package directory, and permits writes only to standard output and standard error. The child receives no inherited environment, external filesystem, network connection, or arbitrary child-process authority.

Package-private files are intentionally readable so an extension can ship data and helper assets. Executable code elsewhere is not allowed. Request and response byte ceilings plus a wall-clock deadline bound each invocation. This launcher is currently macOS-only; the protocol remains platform-neutral.

Apple marks `sandbox-exec` as deprecated. Near therefore treats this launcher as a replaceable platform adapter, keeps the public protocol independent of Seatbelt syntax, and must reassess the launcher before stable distribution or a macOS release that removes the command.

## Compatibility

`protocol` is a SemVer requirement matched against the host protocol version. A compatible 0.1.0 host may tighten validation, diagnostics, or resource ceilings, but cannot change request fields, response effects, or their meaning. Adding brokered capabilities, providers, streaming, or persistent workers requires a new protocol version and compatibility fixtures.
