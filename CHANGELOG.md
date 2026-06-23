# Changelog

## Unreleased

### Make Auth Simpler - API-Key Local Dashboard Login

Date: 2026-06-23 15:28:20 CDT
Task: Update local `pulse dashboard` to use API-key backed server login.
Message: Local users can run `pulse up` then `pulse dashboard` without seeing or managing dashboard login credentials.
Status: Completed
PR: #4
PR URL: https://github.com/EK-LABS-LLC/trace-cli/pull/4

#### Added

- Local `pulse dashboard` now sends configured `api_key`, `project_id`, and `redirect_url` to the local login-token endpoint.

#### Changed

- Local mode inference now treats loopback `api_url` values as local even when old local credential fields are absent.
- Remote mode behavior is unchanged.

#### Fixed

- Removes the local dashboard dependency on saved `local_email` and `local_password`.

#### Removed

- Removed the local dashboard fallback that opened the dashboard unauthenticated when local credentials were missing.

#### Handoff Context

- Branch: `feat/make-auth-simpler`.
- Paired server PR: https://github.com/EK-LABS-LLC/trace-service/pull/7.
- User-facing local flow should be `pulse up` then `pulse dashboard`.
- Verified with `cargo test`.
- `cargo fmt --check` still reports pre-existing formatting differences in unrelated files: `src/commands/install_hooks.rs` and `src/commands/status.rs`.
