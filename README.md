# mcpcall

[![CI](https://github.com/loonghao/mcpcall/actions/workflows/ci.yml/badge.svg)](https://github.com/loonghao/mcpcall/actions/workflows/ci.yml)
[![Release](https://github.com/loonghao/mcpcall/actions/workflows/release.yml/badge.svg)](https://github.com/loonghao/mcpcall/actions/workflows/release.yml)
[![Latest Release](https://img.shields.io/github/v/release/loonghao/mcpcall?sort=semver)](https://github.com/loonghao/mcpcall/releases)
[![Downloads](https://img.shields.io/github/downloads/loonghao/mcpcall/total?label=downloads)](https://github.com/loonghao/mcpcall/releases)
[![License](https://img.shields.io/github/license/loonghao/mcpcall)](Cargo.toml)
[![Rust 1.95.0](https://img.shields.io/badge/rust-1.95.0-orange)](rust-toolchain.toml)

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
prompts, import named MCP server configs, diagnose endpoints, batch calls over
one session, discover OAuth metadata, and run over stdio, Streamable HTTP, or
legacy SSE.

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

## Install

Install `mcpcall` the same way as many other CLI tools: run the installer script
and let it download the matching GitHub Release asset for your platform.
`install.sh` installs to `$HOME/.local/bin` by default, or to
`MCPCALL_INSTALL_DIR` when you want a project-local or CI-local install.

Linux and macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/loonghao/mcpcall/main/scripts/install.sh | sh
```

Windows users can use the PowerShell installer:

```powershell
irm https://raw.githubusercontent.com/loonghao/mcpcall/main/scripts/install.ps1 | iex
```

Pin a version or install into a CI-local directory:

```bash
MCPCALL_VERSION=mcpcall-v0.1.0 MCPCALL_INSTALL_DIR="$RUNNER_TEMP/mcpcall-bin" \
  sh -c "$(curl -fsSL https://raw.githubusercontent.com/loonghao/mcpcall/main/scripts/install.sh)"
```

```powershell
$env:MCPCALL_VERSION = "mcpcall-v0.1.0"
$env:MCPCALL_INSTALL_DIR = Join-Path $env:RUNNER_TEMP "mcpcall-bin"
irm https://raw.githubusercontent.com/loonghao/mcpcall/main/scripts/install.ps1 | iex
```

## Usage

List tools from a DCC MCP HTTP endpoint:

```powershell
mcpcall list --url http://127.0.0.1:8765/mcp
mcpcall list --url http://127.0.0.1:8765/mcp --schema
mcpcall list --url http://127.0.0.1:8765/mcp --json
```

Use a named server from any `mcpServers` JSON config:

```powershell
mcpcall config import --from .\mcp.json --output .\mcpcall.json
mcpcall config list --config .\mcpcall.json
mcpcall list --config .\mcpcall.json --server maya --schema
```

Discover MCP servers already registered with common clients:

```powershell
mcpcall config discover --json
mcpcall config discover --output .\mcpcall.json --merge
```

Discovery scans project and user config locations for Cursor, Claude Code,
Claude Desktop, Codex, Windsurf, OpenCode, and VS Code. It accepts strict JSON,
JSONC/JSON5-style files, and Codex TOML config files.

Config entries accept common remote URL spellings such as `url`, `baseUrl`,
`serverUrl`, `httpUrl`, and `mcpUrl`. Header, env, bearer, command, argument,
and root values can use `${VAR}`, `${VAR:-fallback}`, or `$env:VAR` placeholders.

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

Call a legacy SSE server:

```powershell
mcpcall list --sse-url http://127.0.0.1:8765/sse
```

Diagnose a server before a smoke test:

```powershell
mcpcall doctor --url http://127.0.0.1:8765/mcp --json
```

Reuse one MCP session for several tool calls:

```powershell
@'
[
  {"tool":"dcc_status","arguments":{}},
  {"tool":"scene_summary","arguments":{"include_hidden":false}}
]
'@ | mcpcall batch --url http://127.0.0.1:8765/mcp --json
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

Request completion suggestions when a server supports completions:

```powershell
mcpcall complete --url http://127.0.0.1:8765/mcp prompt review_scene focus mat
mcpcall complete --url http://127.0.0.1:8765/mcp resource "scene://{node}" node cam
```

Generate local automation helpers:

```powershell
mcpcall export --url http://127.0.0.1:8765/mcp types --namespace MayaMcp
mcpcall export --url http://127.0.0.1:8765/mcp shell --shell powershell
```

Environment and working directory for stdio:

```powershell
mcpcall list --stdio "python -m my_mcp_server" --cwd C:\repo --env TOKEN=abc
```

HTTP headers and bearer token:

```powershell
mcpcall list --url https://example.com/mcp --bearer $env:MCP_TOKEN
mcpcall list --url https://example.com/mcp --bearer-env MCP_TOKEN
mcpcall list --url https://example.com/mcp --header X-Trace=local-test
```

Discover OAuth metadata or request a script-friendly client-credentials token:

```powershell
mcpcall auth discover --url https://example.com/mcp --json
mcpcall auth client-credentials --token-url https://auth.example.com/oauth/token --client-id ci --client-secret-env MCP_CLIENT_SECRET --scope mcp:tools
```

Advertise MCP roots to servers that request `roots/list`:

```powershell
mcpcall call --url http://127.0.0.1:8765/mcp --root C:\repo tool_name
```

Argument values are parsed as JSON when possible. Otherwise they are strings.
Use `@path` to load a file as a string value, and `@@value` for a literal value
starting with `@`.

## CI and Releases

The GitHub Actions setup mirrors `canvas-bridge`:

- `CI` runs formatting, clippy, tests, and cross-platform release builds.
- `Release` runs `release-please` on `main`.
- When `release-please` creates a release, CI uploads Linux x86_64/aarch64,
  Windows x86_64, macOS x86_64/aarch64 CLI binaries, target-triple ZIP archives
  such as `mcpcall-<version>-x86_64-pc-windows-msvc.zip`, plus
  `mcpcall-skill.zip`. The original raw binary assets stay published for the
  installer scripts and setup action.

Local preflight:

```powershell
vx just preflight
```

## GitHub Actions

Use the bundled setup action from other GitHub repositories to download the
latest release binary onto `PATH`:

```yaml
- uses: loonghao/mcpcall/.github/actions/setup-mcpcall@main
- run: mcpcall list --url http://127.0.0.1:8765/mcp --json
```

Or use the install script when you want a plain shell step that also works
outside GitHub Actions:

```yaml
- name: Install mcpcall
  run: |
    curl -fsSL https://raw.githubusercontent.com/loonghao/mcpcall/main/scripts/install.sh | sh
- run: mcpcall list --url http://127.0.0.1:8765/mcp --json
```

Pin a release tag when a workflow needs reproducibility:

```yaml
- uses: loonghao/mcpcall/.github/actions/setup-mcpcall@main
  with:
    version: mcpcall-v0.1.0
```

For Windows runners:

```yaml
- name: Install mcpcall
  shell: pwsh
  run: irm https://raw.githubusercontent.com/loonghao/mcpcall/main/scripts/install.ps1 | iex
- run: mcpcall.exe list --url http://127.0.0.1:8765/mcp --json
```

## Agent Skill

Install the bundled skill from `skills/mcpcall` or use the release asset
`mcpcall-skill.zip`. The skill teaches agents to list MCP tools first, inspect
schemas, read resources/prompts, call DCC MCP endpoints safely, and avoid the
conflicting `mcp` command name.
