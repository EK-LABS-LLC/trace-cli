# Pulse CLI

CLI that hooks into Claude Code to capture tool and session events as structured spans, then ships them to the Pulse trace service.

## Quick Start

Requires a running [Pulse trace service](https://github.com/EK-LABS-LLC/trace-service) and a Claude Code installation.

```bash
cargo install --path .
pulse init
pulse connect
```

That's it. Every Claude Code session now sends spans to your trace service.

## Setup

### 1. Build

```bash
cargo build --release
```

Binary is at `target/release/pulse`. Or install directly:

```bash
cargo install --path .
```

### 2. Initialize

```bash
pulse init
```

Prompts for your trace service URL, API key, and project ID. Validates connectivity before saving to `~/.pulse/config.toml`.

For CI/Docker, use flags to skip prompts:

```bash
pulse init \
  --api-url https://pulse.example.com \
  --api-key sk-your-key \
  --project-id my-project \
  --no-validate
```

### 3. Connect Hooks

```bash
pulse connect
```

Installs 10 async hooks into `~/.claude/settings.json`:

```
PreToolUse, PostToolUse, PostToolUseFailure, SessionStart,
SessionEnd, Stop, SubagentStart, SubagentStop,
UserPromptSubmit, Notification
```

Hooks are non-blocking — Claude Code never waits for Pulse.

### 4. Verify

```bash
pulse status
```

Shows config, trace service connectivity, and hook status (e.g. `10/10 hooks installed`).

## Commands

| Command | Description |
|---------|-------------|
| `pulse init` | Configure trace service connection |
| `pulse connect` | Install hooks into Claude Code |
| `pulse disconnect` | Remove all Pulse hooks |
| `pulse status` | Show config, connectivity, and hook status |
| `pulse emit <type>` | Send a span (called by hooks, not by users) |

## How It Works

When Claude Code fires an event (tool call, session start, etc.), it pipes JSON to `pulse emit <event_type>` via the hook. The CLI:

1. Reads the JSON payload from stdin
2. Extracts structured fields based on event type (tool name, input, response, errors, etc.)
3. Builds a span with a UUID, timestamp, and metadata
4. POSTs it to the trace service at `/v1/spans/async`

The `emit` command is designed for the hot path:
- Exits `0` regardless of failures
- Never prints to stdout/stderr
- 2-second HTTP timeout

### Debugging

Set `PULSE_DEBUG=1` to log raw payloads from Claude Code:

```bash
export PULSE_DEBUG=1
```

Writes to `~/.pulse/debug.log` by default. Override with `PULSE_DEBUG_LOG=/path/to/file`.

## Span Schema

Each span sent to the trace service includes:

| Field | Description |
|-------|-------------|
| `span_id` | UUID v4 |
| `session_id` | Claude Code session identifier |
| `timestamp` | ISO 8601 |
| `source` | Always `claude_code` |
| `kind` | `tool_use`, `session`, `agent_run`, `user_prompt`, or `notification` |
| `event_type` | The specific event (e.g. `post_tool_use`, `session_start`) |
| `status` | `success` or `error` (only `post_tool_use_failure` is `error`) |
| `tool_name` | Tool name (for tool events) |
| `tool_input` | Tool input payload (for tool events) |
| `tool_response` | Tool response (for post_tool_use) |
| `error` | Error details (for failures) |
| `cwd` | Working directory |
| `model` | Model name (if provided by the hook source) |
| `agent_name` | Subagent type (for subagent events) |
| `metadata` | Contains `cli_version`, `project_id`, and event-specific data |

## Testing

### Unit Tests

```bash
make test
```

30 tests covering span extraction, hook install/uninstall, and serialization.

### E2E Tests

Runs Claude Code in Docker, fires real hooks, and validates spans land in the trace service with correct structure.

```bash
cp e2e/.env.example e2e/.env
# Fill in ANTHROPIC_API_KEY, PULSE_API_URL, PULSE_API_KEY
make e2e-run
```

Validates 35 assertions: span count, session consistency, UUID format, timestamps, field presence, kind/status mappings, metadata integrity, prompt capture, and cleanup.

### All Make Targets

| Target | Description |
|--------|-------------|
| `make build` | `cargo build` |
| `make test` | `cargo test` |
| `make clean` | `cargo clean` |
| `make e2e-build` | Build the e2e Docker image |
| `make e2e-run` | Build and run e2e tests |
| `make e2e-down` | Tear down e2e containers |

## Project Structure

```
src/
  lib.rs              # Library root
  main.rs             # CLI entrypoint
  config.rs           # ~/.pulse/config.toml management
  error.rs            # Error types
  http.rs             # HTTP client and SpanPayload
  commands/
    init.rs           # pulse init
    connect.rs        # pulse connect
    disconnect.rs     # pulse disconnect
    status.rs         # pulse status
    emit.rs           # pulse emit (hot path)
  hooks/
    mod.rs            # ToolHook trait and HookStatus
    claude_code.rs    # Claude Code hook definitions and settings.json management
    span.rs           # Span extraction and event type dispatch
tests/
  span_test.rs        # Span extraction tests
  http_test.rs        # Serialization tests
e2e/
  Dockerfile          # Multi-stage build (Rust + Node)
  run.sh              # E2E test script
  docker-compose.yml  # E2E orchestration
```
