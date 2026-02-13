# Pulse CLI

Rust CLI that installs non-blocking hooks into agentic tools (starting with Claude Code) so that tool lifecycle events (post tool use, stop, session start, …) flow into the Pulse trace service.

## Getting Started

1. **Build** (from repo root):
   ```bash
   cd cli
   cargo build
   ```
   This produces `target/debug/cli`.
2. **Install** (optional) so `pulse` is on your `$PATH`:
   ```bash
   cargo install --path cli --force
   ```

> The CLI reads/writes `~/.pulse/config.toml` and expects Claude Code settings at `~/.claude/settings.json`.

## Commands

### `pulse init`
Interactive bootstrap. Prompts for:
- Trace service URL (e.g. `https://pulse.example.com`)
- API key (kept locally)
- Project ID

The command pings `/health` before writing config to `~/.pulse/config.toml`.

### `pulse connect`
Ensures hooks are present in supported tools. Today only Claude Code is implemented:
```
~/.claude/settings.json
└── hooks
    ├── PostToolUse
    ├── Stop
    └── SessionStart
```
Each event runs `pulse emit <event_type>` asynchronously so Claude Code never blocks. The command is idempotent and re-runs safely.

### `pulse disconnect`
Removes any `pulse emit …` hook entries and cleans up now-empty arrays or hook sections.

### `pulse status`
Prints current configuration, health-checks the trace service, and shows whether hooks are installed per tool.

### `pulse emit <event_type>`
Hot path invoked by hooks. Reads JSON from STDIN (the payload from Claude Code), wraps it with metadata, and POSTs to `/v1/events/batch`. Design constraints:
- Exits `0` regardless of failures (missing config, invalid JSON, HTTP error, etc.)
- Never prints to stdout/stderr
- 2s HTTP timeout

Example:
```bash
echo '{"session_id":"abc","tool_name":"Terminal","payload":{"cmd":"ls"}}' | \
  pulse emit post_tool_use
```

## Development Notes

- Modules live in `cli/src/commands/*` while common utilities are in `config.rs`, `http.rs`, `hooks/*`, and `error.rs`.
- Hooks are defined via the `ToolHook` trait; add additional implementations under `cli/src/hooks/` and register them in `commands/mod.rs`.
- HTTP requests use `reqwest` with `rustls` to avoid OpenSSL dependencies.
- Async runtime is the single-threaded Tokio flavor for fast startup.

## Testing Checklist

1. `cargo fmt`
2. `cargo clippy --all-targets`
3. `cargo test`
4. Manual workflow: `pulse init` → `pulse connect` → simulate `pulse emit` → verify event in trace service → `pulse status` → `pulse disconnect`

(Online access is required for `cargo` to pull crates the first time.)
