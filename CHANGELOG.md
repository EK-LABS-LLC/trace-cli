# Changelog

## Unreleased

### Emit Agent Hooks As OTLP Traces

Date: 2026-07-06 CDT; Status: Completed; PR: TBD
Task: Send Claude Code, Codex, OpenCode, and OpenClaw hook events through the canonical OTLP HTTP JSON trace ingest path.
Changed: Bumped CLI package version to 0.2.16.
Added/Changed: Hook emission now POSTs OTLP-shaped data to `/v1/traces`, uses `agent.turn` for user prompt turns, attaches turn events with OTel trace/span/parent IDs when active-turn state is available, and preserves session naming in `pulse.session.name`.

### Add Component-Aware Update Prompt

Date: 2026-06-28 00:00 CDT; Status: Completed; PR: #11 https://github.com/EK-LABS-LLC/trace-cli/pull/11
Task: Prompt when CLI or local server/dashboard releases are stale.
Changed: Bumped CLI package version to 0.2.15.
Added: `pulse update`, startup update prompts, and per-version dismissal.

### Fix CLI Version Guard Release Baseline

Date: 2026-06-24 21:10 CDT; Status: Completed; PR: #10 https://github.com/EK-LABS-LLC/trace-cli/pull/10
Task: Make the CLI version guard compare against released tags, not only main.
Changed: Bumped CLI package version to 0.2.14.
Fixed: PR CI now fails unless Cargo.toml/Cargo.lock are above both main and latest release.

### Guard CLI Release Versions

Date: 2026-06-24 18:09 CDT; Status: Completed; PR: #8 https://github.com/EK-LABS-LLC/trace-cli/pull/8
Task: Prevent CLI PRs from merging with a stale package version.
Changed: Bumped CLI package version to 0.2.12.
Added: PR CI now checks that Cargo.toml is bumped above main and matches Cargo.lock.

### Add Codex Hook Support

Date: 2026-06-23 16:51:17 CDT; Status: Completed; PR: TBD
Task: Add Codex as a first-class Pulse hook integration.
Message: `pulse install-hooks` now writes Codex lifecycle hooks and a hidden `emit-codex` adapter labels Codex spans correctly.
Added/Changed: Codex status/connect/disconnect support, `permission_request` span mapping, and README hook docs.

### Fix Claude Hook Install Location

Date: 2026-06-23 16:59:58 CDT; Status: Completed; PR: TBD
Task: Prevent Claude Code hooks from being installed into a settings file Claude does not load.
Message: Claude hooks now install to `~/.claude/settings.json`, use the absolute running `pulse` binary path, and migrate old misplaced Pulse hooks out of `settings.local.json`.
Fixed/Changed: `pulse status`, `install-hooks`, and `disconnect` now recognize migrated/absolute Claude hook commands.

### Fix CLI Release Version And Installer Start Text

Date: 2026-06-23 16:34:27 CDT; Status: Completed; PR: TBD
Task: Fix post-release CLI metadata and install guidance.
Message: `pulse --version` now matches the next release, and installer output points local users to `pulse up` then `pulse dashboard`.
Changed/Fixed: Bumped Cargo package version to 0.2.11 and removed stale `pulse setup --local` from install quick start.

### Make Auth Simpler - Final Local Smoke And Tests

Date: 2026-06-23 15:44:20 CDT; Status: Completed; PR: #4 https://github.com/EK-LABS-LLC/trace-cli/pull/4
Task: Finish auth simplification verification and lock no-credential config behavior.
Message: Fresh local bootstrap, dashboard auto-login, API ingest/readback, and SDK connectivity were smoke-tested against local trace-service.
Added/Changed: Added config serialization test proving missing local credentials are omitted from saved TOML.
Fixed/Removed: No new local configs store `local_email` or `local_password`; docs already point users to `pulse up` then `pulse dashboard`.
Handoff: Paired trace-service PR #7; provider SDK calls skipped because no real OpenAI/Anthropic keys were configured.

### Make Auth Simpler - Stop Storing Local Credentials

Date: 2026-06-23 15:44:20 CDT; Status: Completed; PR: #4 https://github.com/EK-LABS-LLC/trace-cli/pull/4
Task: Stop writing local dashboard email/password fields into new local configs.
Message: Local bootstrap still creates/reuses the account internally, but saved config now only needs API URL, API key, project id, and server command.
Added/Changed: README now documents `pulse up` then `pulse dashboard` as the normal local flow.
Fixed/Removed: New local configs no longer persist `local_email` or `local_password`; old configs remain readable.
Handoff: Pair with trace-service PR #7 API-key local login; verify full local smoke after both PRs are reviewed.

### Make Auth Simpler - API-Key Local Dashboard Login

Date: 2026-06-23 15:28:20 CDT; Status: Completed; PR: #4 https://github.com/EK-LABS-LLC/trace-cli/pull/4
Task: Update local `pulse dashboard` to use API-key backed server login.
Message: Local users can run `pulse up` then `pulse dashboard` without seeing or managing dashboard login credentials.
Added/Changed: `pulse dashboard` sends configured `api_key`, `project_id`, and `redirect_url`; loopback API URLs now infer local mode without old credential fields.
Fixed/Removed: Removed dependency on `local_email`/`local_password` and the unauthenticated local dashboard fallback; remote mode is unchanged.
Handoff: Paired server PR #7; verified with `cargo test`; `cargo fmt --check` still has unrelated pre-existing diffs in `install_hooks.rs` and `status.rs`.
