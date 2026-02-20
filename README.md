# Pulse CLI

CLI that hooks into AI coding agents to capture tool and session events as structured spans, then ships them to the Pulse trace service.

Supported agents:
- **Claude Code** — hooks via `~/.claude/settings.json`
- **OpenCode** — plugin via `~/.config/opencode/plugin/`
- **OpenClaw** — hook via `~/.openclaw/hooks/`

## Install

Requires a running [Pulse trace service](https://github.com/EK-LABS-LLC/trace-service) and at least one supported agent installed.

```bash
curl -fsSL https://raw.githubusercontent.com/EK-LABS-LLC/trace-cli/main/install.sh | sh
```

This auto-detects your OS and architecture, downloads the latest release binary, and installs it to `~/.local/bin/pulse`.

**Options:**

```bash
# Install a specific version
curl -fsSL ... | PULSE_VERSION=v0.1.0 sh

# Install to a custom directory
curl -fsSL ... | PULSE_INSTALL_DIR=/usr/local/bin sh
```

## Quick Start

```bash
pulse init        # configure trace service connection
pulse connect     # install hooks into detected agents
pulse status      # verify everything is wired up
```

That's it. Pulse auto-detects which agents are installed and wires them up. Every session now sends spans to your trace service.

## Commands

| Command | Description |
|---------|-------------|
| `pulse init` | Configure trace service connection |
| `pulse connect` | Install hooks into all detected agents |
| `pulse disconnect` | Remove all Pulse hooks from all agents |
| `pulse status` | Show config, connectivity, and hook status |
| `pulse emit <type>` | Send a span (called by hooks, not by users) |

### `pulse init`

```bash
# Interactive (prompts for each value)
pulse init

# Non-interactive (CI/Docker)
pulse init \
  --api-url https://pulse.example.com \
  --api-key sk-your-key \
  --project-id my-project \
  --no-validate
```

Validates connectivity before saving to `~/.pulse/config.toml`.

### `pulse connect`

```bash
pulse connect
```

Auto-detects installed agents and wires up instrumentation:

- **Claude Code** — installs 10 async hooks into `~/.claude/settings.json` (PreToolUse, PostToolUse, PostToolUseFailure, SessionStart, SessionEnd, Stop, SubagentStart, SubagentStop, UserPromptSubmit, Notification)
- **OpenCode** — installs a TypeScript plugin at `~/.config/opencode/plugin/pulse-plugin.ts` that hooks into session, message, and tool events
- **OpenClaw** — installs a hook at `~/.openclaw/hooks/pulse-hook/` that hooks into command and message events

All hooks are non-blocking — your agent never waits for Pulse.

### `pulse status`

```bash
pulse status
```

Shows config, trace service connectivity, and hook status for each detected agent.

## How It Works

When an agent fires an event (tool call, session start, etc.), it pipes JSON to `pulse emit <event_type>`. The CLI:

1. Reads the JSON payload from stdin
2. Extracts structured fields based on event type
3. Builds a span with a UUID, timestamp, and metadata
4. POSTs it to the trace service at `/v1/spans/async`

**Claude Code** calls `pulse emit` directly from its hook system.
**OpenCode** runs a plugin that calls `Bun.spawn(["pulse", "emit", ...])`.
**OpenClaw** runs a handler that calls `child_process.spawn("pulse", ["emit", ...])`.

The `emit` command is designed for the hot path:
- Exits `0` regardless of failures
- Never prints to stdout/stderr
- 2-second HTTP timeout

### Debugging

```bash
export PULSE_DEBUG=1
```

Logs raw payloads to `~/.pulse/debug.log`. Override path with `PULSE_DEBUG_LOG=/path/to/file`.

## Span Schema

Each span sent to the trace service includes:

| Field | Description |
|-------|-------------|
| `span_id` | UUID v4 |
| `session_id` | Agent session identifier |
| `timestamp` | ISO 8601 |
| `source` | `claude_code`, `opencode`, or `openclaw` |
| `kind` | `tool_use`, `session`, `agent_run`, `user_prompt`, `llm_response`, or `notification` |
| `event_type` | The specific event (e.g. `post_tool_use`, `session_start`) |
| `status` | `success` or `error` |
| `tool_name` | Tool name (tool events only) |
| `tool_input` | Tool input payload (tool events only) |
| `tool_response` | Tool response (`post_tool_use` only) |
| `error` | Error details (failures only) |
| `cwd` | Working directory |
| `model` | Model name |
| `agent_name` | Subagent type (subagent events only) |
| `metadata` | Contains `cli_version`, `project_id`, and event-specific data |

## Local Development

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- A running [Pulse trace service](https://github.com/EK-LABS-LLC/trace-service) (for integration/e2e testing)

### Build

```bash
make build          # debug build
make release        # release build
make install        # release build + copy to ~/.local/bin/pulse
```

Or with cargo directly:

```bash
cargo build --release
cargo install --path .
```

### Test

```bash
make test           # unit + integration tests
```

### E2E Tests

E2E tests run each agent in a container, fire real hooks, and validate spans land in the trace service with correct structure.

```bash
# 1. Set up environment
cp e2e/.env.example e2e/.env
# Fill in ANTHROPIC_API_KEY, PULSE_API_URL, PULSE_API_KEY

# 2. Run all suites
make e2e

# Or run individually
make e2e-claude            # Claude Code basic session
make e2e-claude-tools      # Claude Code with tool calls + subagents
make e2e-opencode          # OpenCode basic session
make e2e-opencode-tools    # OpenCode with tool calls

# Tear down
make e2e-down
```

### Make Targets

| Target | Description |
|--------|-------------|
| `make build` | Debug build |
| `make release` | Release build |
| `make test` | Run all tests |
| `make install` | Build release + install to `~/.local/bin` |
| `make clean` | Clean build artifacts |
| `make e2e` | Run all e2e test suites |
| `make e2e-claude` | Claude Code basic e2e |
| `make e2e-claude-tools` | Claude Code tools e2e |
| `make e2e-opencode` | OpenCode basic e2e |
| `make e2e-opencode-tools` | OpenCode tools e2e |
| `make e2e-build` | Build e2e Docker images |
| `make e2e-down` | Tear down e2e containers |

## Project Structure

```
src/
  main.rs               # CLI entrypoint
  lib.rs                 # Library root
  config.rs              # ~/.pulse/config.toml management
  error.rs               # Error types
  http.rs                # HTTP client and SpanPayload
  commands/
    init.rs              # pulse init
    connect.rs           # pulse connect
    disconnect.rs        # pulse disconnect
    status.rs            # pulse status
    emit.rs              # pulse emit (hot path)
  hooks/
    mod.rs               # ToolHook trait and HookStatus
    claude_code.rs       # Claude Code settings.json management
    opencode.rs          # OpenCode plugin management
    openclaw.rs          # OpenClaw hook management
    span.rs              # Span extraction and event type dispatch
plugins/
  opencode/
    pulse-plugin.ts      # OpenCode TypeScript plugin
  openclaw/
    HOOK.md              # OpenClaw hook metadata
    handler.ts           # OpenClaw TypeScript handler
tests/
  span_test.rs           # Span extraction tests
  http_test.rs           # Serialization tests
e2e/
  docker-compose.yml     # E2E orchestration
  Dockerfile             # Claude Code e2e image
  Dockerfile.opencode    # OpenCode e2e image
install.sh               # curl | sh installer
.github/
  workflows/
    release.yml          # Build + release on tag push
```

## Releasing

Releases are automated via GitHub Actions. Push a tag to build and publish:

```bash
git tag v0.1.0
git push origin v0.1.0
```

This builds binaries for Linux (amd64, arm64) and macOS (amd64, arm64), then creates a GitHub release with all artifacts attached.
