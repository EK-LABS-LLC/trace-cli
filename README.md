# Pulse CLI

CLI that hooks into AI coding agents to capture tool and session events as structured spans, then ships them to the Pulse trace service.

Supported agents:
- **Claude Code** — hooks via `~/.claude/settings.json`
- **OpenCode** — plugin via `~/.config/opencode/plugin/`
- **OpenClaw** — hook via `~/.openclaw/hooks/`

## Getting Started

Requires a running [Pulse trace service](https://github.com/EK-LABS-LLC/trace-service) and at least one supported agent installed.

### 1. Install

```bash
curl -fsSL https://raw.githubusercontent.com/EK-LABS-LLC/trace-cli/main/install.sh | sh
```

### 2. Configure

```bash
pulse init
```

You'll be prompted for your trace service URL, API key, and project ID.

### 3. Connect

```bash
pulse connect
```

Pulse auto-detects your installed agents (Claude Code, OpenCode, OpenClaw) and hooks into them.

### 4. Verify

```bash
pulse status
```

You're all set. Every agent session now sends traces automatically.

### Uninstall

```bash
pulse disconnect
rm ~/.local/bin/pulse
rm -rf ~/.pulse
```

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

## Releasing

Releases are automated via GitHub Actions. Push a tag to build and publish:

```bash
git tag v0.1.0
git push origin v0.1.0
```

This builds binaries for Linux (amd64, arm64) and macOS (amd64, arm64), then creates a GitHub release with all artifacts attached.
