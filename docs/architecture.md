# Architecture

`mcpcall` is split around stable contracts instead of SDK details.

## Crates

- `mcpcall` owns the CLI surface, process exit codes, and user-facing command
  routing.
- `mcpcall-core` owns value parsing, transport options, output rendering, and
  DTOs used as the contract between the CLI and adapters.
- `mcpcall-rmcp` owns the concrete `rmcp` integration: Streamable HTTP, legacy
  SSE, stdio, SDK type conversion, timeout handling, and process launching.

## Contract Rules

- The binary crate must not import `rmcp` directly.
- `mcpcall-core` must not import `clap`, `rmcp`, `tokio`, or transport SDKs.
- New MCP primitive support starts by adding a core DTO and renderer, then the
  adapter conversion, then the CLI command.
- CI-oriented output should have a `--json` path before adding more human
  formatting.
- Config-driven flows must preserve compatibility with common `mcpServers` JSON
  files before adding mcpcall-specific fields. Remote URL aliases and
  environment placeholders are normalized in `mcpcall-core`.
- Auth helpers remain CLI-owned unless they become part of a transport contract;
  token discovery and exchange should feed standard bearer-token transport
  options.
- Session reuse should prefer explicit batch/script commands for CI; any future
  daemon must keep the single-command non-interactive workflow intact.
- Release artifacts keep the `mcpcall` binary name so downstream DCC workflows
  can download one file and put it on `PATH`.

## DCC Test Usage

The first downstream contract is non-interactive test automation for
`dcc-mcp-core`, `dcc-mcp-maya`, `dcc-mcp-blender`, and sibling adapters. Commands
must therefore be deterministic, scriptable, and fail with meaningful exit codes.

