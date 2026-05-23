---
name: mcpcall
description: Call MCP servers from shell commands through the mcpcall CLI over stdio or Streamable HTTP. Use when an agent needs to list tools, inspect schemas, or invoke tools on DCC MCP servers such as dcc-mcp-core, dcc-mcp-maya, dcc-mcp-3dsmax, or any generic MCP endpoint.
---

# mcpcall

Use `mcpcall` when the host can run shell commands and should interact with an
MCP server without registering that server directly with the agent runtime.

## Critical Rules

- Use `mcpcall`, not `mcp`; `mcp` commonly belongs to other MCP tooling.
- Start with `mcpcall list` before any `mcpcall call` unless the exact tool name
  and schema were just shown in the current conversation.
- Prefer `--json` when another script or agent step will consume the output.
- For DCC scene-mutating tools, inspect the schema first and tell the user which
  endpoint and tool you are about to call.
- If a DCC host restarts, re-run `mcpcall list`; old tools or endpoints may be stale.

## Quick Start

List tools on a Streamable HTTP MCP endpoint:

```bash
mcpcall list --url http://127.0.0.1:8765/mcp
mcpcall list --url http://127.0.0.1:8765/mcp --schema
```

Call a tool:

```bash
mcpcall call --url http://127.0.0.1:8765/mcp maya_primitives__create_sphere radius=2 name=Ball
```

Use JSON arguments for complex payloads:

```bash
mcpcall call --url http://127.0.0.1:8765/mcp execute_python --args '{"code":"print(\"ok\")"}'
```

Use stdio for local MCP servers:

```bash
mcpcall list --stdio "python -m my_mcp_server"
mcpcall call --stdio "python -m my_mcp_server" my_tool key=value
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
3. Run `mcpcall list --url <url> --schema` and inspect the target tool.
4. Call the tool with either `KEY=VALUE` pairs or `--args` JSON.
5. If the result has `isError=true`, treat the action as failed even if the CLI
   printed content; rerun with `--json` when the raw envelope is needed.
