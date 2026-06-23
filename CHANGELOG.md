# Changelog

## Unreleased

### Local auth simplification

- Updated `pulse dashboard` local mode to request a dashboard login URL with the configured API key and project id.
- Removed the local dashboard auto-login dependency on stored `local_email` and `local_password`.
- Local configs are now inferred as local when `api_url` points at a loopback host, even if old local credential fields are absent.
- Remote mode behavior is unchanged; `pulse dashboard` still opens the configured dashboard URL without local token handoff.

### Handoff context

- This is part of the `make auth simpler` effort on branch `feat/make-auth-simpler`.
- Paired server work adds the loopback-only API-key local login-token endpoint behavior.
- User-facing local flow should be `pulse up` then `pulse dashboard`; users should not need to see or manage dashboard login credentials locally.
- Verified with `cargo test`.
- `cargo fmt --check` still reports pre-existing formatting differences in unrelated files: `src/commands/install_hooks.rs` and `src/commands/status.rs`.
