# Pulse CLI

CLI that hooks into AI coding agents to capture tool and session events as structured spans, then ships them to the Pulse trace service.

Supported agents:
- **Claude Code** — hooks via `~/.claude/settings.json`
- **Codex** — hooks via `~/.codex/hooks.json`
- **OpenCode** — plugin via `~/.config/opencode/plugin/`
- **OpenClaw** — hook via `~/.openclaw/hooks/`

## Getting Started

Requires a running [Pulse trace service](https://github.com/EK-LABS-LLC/trace-service) and at least one supported agent installed.

### 1. Install

Recommended (installs both `pulse-server` and `pulse`):

```bash
curl -fsSL https://raw.githubusercontent.com/EK-LABS-LLC/trace-service/main/scripts/install.sh | bash -s -- pulse-server
```

CLI-only install (if the server is already provisioned separately):

```bash
curl -fsSL https://raw.githubusercontent.com/EK-LABS-LLC/trace-cli/main/install.sh | sh
```

Re-running the installer upgrades the CLI in place and preserves your existing `~/.pulse` config.

### 2. Configure

#### Local managed Pulse (recommended)

```bash
pulse up
pulse dashboard
```

On first run, `pulse up`:
- starts `pulse-server`
- creates/reuses a local dashboard account
- creates/reuses your project API key
- writes `~/.pulse/config.toml`

Then `pulse dashboard` opens the local dashboard with a one-time local login URL.

Daily use:
- run `pulse up` to start `pulse-server` in the background
- run `pulse dashboard` to open the local dashboard
- run `pulse logs --follow` to watch server logs
- run `pulse down` to stop it

#### Remote/shared Pulse instance

```bash
pulse connect \
  --api-url https://pulse.example.com \
  --api-key pulse_sk_... \
  --project-id your-project
```

This saves a remote config and installs hooks locally. It does not start a server.

### 3. Verify

```bash
pulse status
```

You're all set. Every agent session now sends traces automatically.

### Updating

Local/self-hosted Pulse:

```bash
curl -fsSL https://raw.githubusercontent.com/EK-LABS-LLC/trace-service/main/scripts/install.sh | bash -s -- pulse-server
```

This updates `pulse-server`, dashboard assets, and the `pulse` CLI while preserving `~/.pulse` data.

Remote/shared-instance CLI users:

```bash
curl -fsSL https://raw.githubusercontent.com/EK-LABS-LLC/trace-cli/main/install.sh | sh
```

You can also run:

```bash
pulse update
```

For local managed installs, `pulse update` checks the latest `pulse-server`/dashboard release and the latest CLI release. If any part is out of date, it updates the server binary, dashboard assets, and CLI in place without removing `~/.pulse` data. If the server is already running, restart it after updating:

```bash
pulse restart
```

For remote/shared-instance configs, `pulse update` updates only the CLI. Interactive commands also prompt when an update is available. Set `PULSE_SKIP_UPDATE_CHECK=1` to skip the automatic check.

### Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/EK-LABS-LLC/trace-service/main/scripts/uninstall.sh | bash

# full cleanup (config + local data too)
curl -fsSL https://raw.githubusercontent.com/EK-LABS-LLC/trace-service/main/scripts/uninstall.sh | bash -s -- --purge-data
```

## Commands

| Command | Description |
|---------|-------------|
| `pulse setup` | Manually bootstrap a local or remote Pulse account/project and save config |
| `pulse up` | Start the managed local Pulse server in the background |
| `pulse down` | Stop the managed local Pulse server |
| `pulse restart` | Restart the managed local Pulse server |
| `pulse logs` | Show or follow managed local server logs |
| `pulse dashboard` | Open the current Pulse dashboard URL |
| `pulse update` | Update stale local Pulse components |
| `pulse init` | Deprecated alias for `pulse connect` |
| `pulse connect` | Configure a remote Pulse instance and install hooks |
| `pulse disconnect` | Remove all Pulse hooks from all agents |
| `pulse status` | Show mode, connectivity, server state, and hook status |
| `pulse emit <type>` | Send a span (called by hooks, not by users) |

### `pulse setup`

```bash
# Manual local bootstrap
pulse setup --local

# Fully non-interactive
pulse setup \
  --api-url http://localhost:3000 \
  --name "Your Name" \
  --email you@example.com \
  --password "change-me" \
  --project-name "My Project"
```

If your server is already running elsewhere and you want setup to create/sign in
an account against that remote instance, use:

```bash
pulse setup --api-url https://pulse.example.com --no-start-server
```

Show full API key in setup output:

```bash
pulse setup --local --show-api-key
```

For normal local use, `pulse up` can perform first-time bootstrap automatically:

```bash
pulse up
```

### `pulse up`

```bash
pulse up

# Start and open the dashboard immediately
pulse up --open
```

Starts `pulse-server` in the background, waits for `/health`, performs first-time
local bootstrap if config is missing, and prints the dashboard URL, PID, and log
path.

### `pulse down`

```bash
pulse down
```

Stops the managed local `pulse-server`.

### `pulse restart`

```bash
pulse restart

# Restart and reopen the dashboard
pulse restart --open
```

### `pulse logs`

```bash
# Show the most recent log lines
pulse logs

# Follow logs continuously
pulse logs --follow

# Show a larger tail before following
pulse logs --lines 300 --follow
```

### `pulse dashboard`

```bash
# Open the configured dashboard
pulse dashboard

# Print the URL instead of opening a browser
pulse dashboard --no-open
```

In local mode, this uses the one-time local auto-login handoff. In remote mode,
it just opens the configured remote dashboard URL.

### `pulse init`

```bash
# Deprecated alias for `pulse connect`
pulse init
```

### `pulse connect`

```bash
pulse connect \
  --api-url https://pulse.example.com \
  --api-key pulse_sk_... \
  --project-id my-project
```

Interactive prompts work too:

```bash
pulse connect
```

This:
- saves a remote-mode config to `~/.pulse/config.toml`
- validates `/health` by default
- installs hooks into detected agents unless `--no-hooks` is set

Hook installation covers:

- **Claude Code** — installs 10 async hooks into `~/.claude/settings.json` (PreToolUse, PostToolUse, PostToolUseFailure, SessionStart, SessionEnd, Stop, SubagentStart, SubagentStop, UserPromptSubmit, Notification)
- **Codex** — installs 8 lifecycle hooks into `~/.codex/hooks.json` (SessionStart, UserPromptSubmit, PreToolUse, PermissionRequest, PostToolUse, SubagentStart, SubagentStop, Stop). Run `/hooks` in Codex to review and trust new hooks.
- **OpenCode** — installs a TypeScript plugin at `~/.config/opencode/plugin/pulse-plugin.ts` that hooks into session, message, and tool events
- **OpenClaw** — installs a hook at `~/.openclaw/hooks/pulse-hook/` that hooks into command and message events

All hooks are non-blocking — your agent never waits for Pulse.

### `pulse status`

```bash
pulse status
```

Shows:
- current mode (`local` or `remote`)
- configured API URL and project
- local server PID / health / log path in local mode
- remote connectivity in remote mode
- hook status for each detected agent

## How It Works

When an agent fires an event (tool call, session start, etc.), it pipes JSON to `pulse emit <event_type>`. The CLI:

1. Reads the JSON payload from stdin
2. Extracts structured fields based on event type
3. Builds a span with a UUID, timestamp, and metadata
4. POSTs it to the trace service at `/v1/spans/async`

**Claude Code** calls `pulse emit` directly from its hook system.
**Codex** calls `pulse emit-codex` from its lifecycle hooks, which normalizes Codex hook payloads into Pulse spans.
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
| `source` | `claude_code`, `codex`, `opencode`, or `openclaw` |
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
# Fill in ANTHROPIC_API_KEY, OPENAI_API_KEY, PULSE_API_URL, PULSE_API_KEY

# 2. Run all suites
make e2e

# Or run individually
make e2e-claude            # Claude Code basic session
make e2e-claude-tools      # Claude Code with tool calls + subagents
make e2e-opencode          # OpenCode basic session
make e2e-opencode-tools    # OpenCode with tool calls
make e2e-codex             # Codex basic session
make e2e-codex-tools       # Codex with tool calls

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
