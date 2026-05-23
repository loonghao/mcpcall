# mcpcall

`mcpcall` is a Rust CLI for exercising MCP servers from shell scripts and CI. It
is meant for DCC MCP smoke tests around `dcc-mcp-core`, `dcc-mcp-maya`,
`dcc-mcp-blender`, `dcc-mcp-3dsmax`, and related adapters, while staying generic
enough for any MCP server.

## Existing Options Checked

- `openclaw/mcporter`: TypeScript package with rich config import, friendly
  function-call syntax, daemon support, OAuth, and generated CLIs.
- `wong2/mcp-cli`: JavaScript inspector CLI for tools, resources, prompts, and
  OAuth-backed HTTP transports.
- `mcp-cli` / `mcp-probe`: Rust debugger and TUI for MCP servers.
- `rmcp`: official Rust SDK. `mcpcall` uses this for protocol and transport
  support instead of hand-rolling JSON-RPC.

The current local goal is a focused non-interactive CLI that can replace
`mcporter` in DCC test automation: list/call tools, list/read resources, get
prompts, and run over stdio or Streamable HTTP.

## Architecture

The repository is a Cargo workspace with narrow crate responsibilities:

- `mcpcall` is the binary crate. It owns command-line parsing and exit codes.
- `mcpcall-core` owns domain contracts, argument parsing, transport options, and
  output rendering. It has no dependency on `rmcp` or `clap`.
- `mcpcall-rmcp` is the protocol adapter. It owns `rmcp` transports and converts
  SDK types into `mcpcall-core` contracts.

This keeps the CLI interface small, the protocol adapter replaceable, and the
test surface centered on contracts instead of SDK details.

## Build

```powershell
cargo build --workspace
```

The release binary is named `mcpcall` (`mcpcall.exe` on Windows). Do not rename
it to `mcp` by default; that command name is already used by other MCP tooling.

## Usage

List tools from a DCC MCP HTTP endpoint:

```powershell
mcpcall list --url http://127.0.0.1:8765/mcp
mcpcall list --url http://127.0.0.1:8765/mcp --schema
mcpcall list --url http://127.0.0.1:8765/mcp --json
```

Call a tool:

```powershell
mcpcall call --url http://127.0.0.1:8765/mcp maya_primitives__create_sphere radius=2 name=ball
mcpcall call --url http://127.0.0.1:8765/mcp maya_primitives__create_sphere --args '{"radius":2,"name":"ball"}'
mcpcall call --url http://127.0.0.1:8765/mcp 'maya_primitives__create_sphere(radius: 2, name: "ball")'
```

Call a stdio MCP server:

```powershell
mcpcall list --stdio "python -m my_mcp_server"
mcpcall call --stdio "python -m my_mcp_server" my_tool key=value
```

List and read resources:

```powershell
mcpcall resources --url http://127.0.0.1:8765/mcp list
mcpcall resources --url http://127.0.0.1:8765/mcp templates
mcpcall resources --url http://127.0.0.1:8765/mcp read file:///scene/status.json
```

List and get prompts:

```powershell
mcpcall prompts --url http://127.0.0.1:8765/mcp list
mcpcall prompts --url http://127.0.0.1:8765/mcp get review_scene focus=materials
mcpcall prompts --url http://127.0.0.1:8765/mcp get review_scene --args '{"focus":"materials"}'
```

Environment and working directory for stdio:

```powershell
mcpcall list --stdio "python -m my_mcp_server" --cwd C:\repo --env TOKEN=abc
```

HTTP headers and bearer token:

```powershell
mcpcall list --url https://example.com/mcp --bearer $env:MCP_TOKEN
mcpcall list --url https://example.com/mcp --header X-Trace=local-test
```

Argument values are parsed as JSON when possible. Otherwise they are strings.
Use `@path` to load a file as a string value, and `@@value` for a literal value
starting with `@`.

## CI and Releases

The GitHub Actions setup mirrors `canvas-bridge`:

- `CI` runs formatting, clippy, tests, and cross-platform release builds.
- `Release` runs `release-please` on `main`.
- When `release-please` creates a release, CI uploads Linux, Windows, and macOS
  CLI binaries plus `mcpcall-skill.zip`.

Local preflight:

```powershell
vx just preflight
```

## GitHub Actions

Use the bundled setup action from other repositories to download the latest
release binary onto `PATH`:

```yaml
- uses: loonghao/mcpcall/.github/actions/setup-mcpcall@main
- run: mcpcall list --url http://127.0.0.1:8765/mcp --json
```

Pin a release tag when a workflow needs reproducibility:

```yaml
- uses: loonghao/mcpcall/.github/actions/setup-mcpcall@main
  with:
    version: mcpcall-v0.1.0
```

## Agent Skill

Install the bundled skill from `skills/mcpcall` or use the release asset
`mcpcall-skill.zip`. The skill teaches agents to list MCP tools first, inspect
schemas, read resources/prompts, call DCC MCP endpoints safely, and avoid the
conflicting `mcp` command name.
