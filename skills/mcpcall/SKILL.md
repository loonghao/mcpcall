---
name: mcpcall
description: Call MCP servers from shell commands through the mcpcall CLI over stdio, Streamable HTTP, or legacy SSE. Use when an agent needs to list tools, inspect schemas, read resources/prompts, diagnose endpoints, batch tool calls, or invoke tools on DCC MCP servers such as dcc-mcp-core, dcc-mcp-maya, dcc-mcp-blender, dcc-mcp-3dsmax, or any generic MCP endpoint.
---

# mcpcall

Use `mcpcall` when the host can run shell commands and should interact with an
MCP server without registering that server directly with the agent runtime.

## Critical Rules

- Use `mcpcall`, not `mcp`; `mcp` commonly belongs to other MCP tooling.
- Start with `mcpcall list` before any `mcpcall call` unless the exact tool name
  and schema were just shown in the current conversation.
- Use `mcpcall resources ...` or `mcpcall prompts ...` when the task is about
  MCP context primitives instead of executable tools.
- Use `mcpcall doctor` before debugging flaky endpoint failures.
- Prefer `--json` when another script or agent step will consume the output.
- Use `mcpcall batch` when several tool calls should share one MCP session.
- Use `mcpcall auth discover` for OAuth-backed HTTP servers before guessing
  token endpoints.
- For DCC scene-mutating tools, inspect the schema first and tell the user which
  endpoint and tool you are about to call.
- If a DCC host restarts, re-run `mcpcall list`; old tools or endpoints may be stale.

## Quick Start

List tools on a Streamable HTTP MCP endpoint:

```bash
mcpcall list --url http://127.0.0.1:8765/mcp
mcpcall list --url http://127.0.0.1:8765/mcp --schema
```

Use a named server from a config file:

```bash
mcpcall config import --from ./mcp.json --output ./mcpcall.json
mcpcall list --config ./mcpcall.json --server maya --schema
```

Config values may use common aliases like `baseUrl`/`serverUrl` and environment
placeholders such as `${MCP_TOKEN}`, `${MCP_TOKEN:-fallback}`, or `$env:MCP_TOKEN`.

Call a tool:

```bash
mcpcall call --url http://127.0.0.1:8765/mcp maya_primitives__create_sphere radius=2 name=Ball
```

Use JSON arguments for complex payloads:

```bash
mcpcall call --url http://127.0.0.1:8765/mcp execute_python --args '{"code":"print(\"ok\")"}'
```

Call several tools over one session:

```bash
mcpcall batch --url http://127.0.0.1:8765/mcp --file smoke.json --json
```

Diagnose endpoint capabilities:

```bash
mcpcall doctor --url http://127.0.0.1:8765/mcp --json
```

Read resources and prompts:

```bash
mcpcall resources --url http://127.0.0.1:8765/mcp list
mcpcall resources --url http://127.0.0.1:8765/mcp read file:///scene/status.json
mcpcall prompts --url http://127.0.0.1:8765/mcp list
mcpcall prompts --url http://127.0.0.1:8765/mcp get review_scene focus=materials
```

Use stdio for local MCP servers:

```bash
mcpcall list --stdio "python -m my_mcp_server"
mcpcall call --stdio "python -m my_mcp_server" my_tool key=value
```

Use legacy SSE when a server exposes `/sse` instead of Streamable HTTP:

```bash
mcpcall list --sse-url http://127.0.0.1:8765/sse
```

Generate helper types or shell wrappers:

```bash
mcpcall export --url http://127.0.0.1:8765/mcp types
mcpcall export --url http://127.0.0.1:8765/mcp shell --shell bash
```

Discover remote auth metadata:

```bash
mcpcall auth discover --url https://example.com/mcp --json
```

## Argument Syntax

`mcpcall` accepts these equivalent forms:

```bash
mcpcall call --url http://127.0.0.1:8765/mcp tool_name key=value
mcpcall call --url http://127.0.0.1:8765/mcp tool_name --arg key=value
mcpcall call --url http://127.0.0.1:8765/mcp tool_name --args '{"key":"value"}'
mcpcall call --url http://127.0.0.1:8765/mcp 'tool_name(key: "value")'
```

Values are parsed as JSON when possible, so `true`, `3`, `2.5`, arrays, and
objects become structured values. Otherwise they are strings.

Use `@path` to load a file as a string argument. Use `@@literal` to pass a value
that begins with `@`.

## Recommended DCC Workflow

1. Determine the MCP URL for the running DCC host.
2. Run `mcpcall list --url <url> --brief` to confirm connectivity.
3. Run `mcpcall doctor --url <url> --json` if connectivity or capabilities are
   unclear.
4. Run `mcpcall list --url <url> --schema` and inspect the target tool.
5. Use `mcpcall resources` or `mcpcall prompts` if the needed context is not a
   tool.
6. Call the tool with either `KEY=VALUE` pairs or `--args` JSON.
7. If multiple calls are part of one smoke test, put them in a JSON file and use
   `mcpcall batch`.
8. If the result has `isError=true`, treat the action as failed even if the CLI
   printed content; rerun with `--json` when the raw envelope is needed.
