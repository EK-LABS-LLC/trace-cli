# Changelog

## Unreleased

### Make Auth Simpler - API-Key Local Dashboard Login

Date: 2026-06-23 15:28:20 CDT; Status: Completed; PR: #4 https://github.com/EK-LABS-LLC/trace-cli/pull/4
Task: Update local `pulse dashboard` to use API-key backed server login.
Message: Local users can run `pulse up` then `pulse dashboard` without seeing or managing dashboard login credentials.
Added/Changed: `pulse dashboard` sends configured `api_key`, `project_id`, and `redirect_url`; loopback API URLs now infer local mode without old credential fields.
Fixed/Removed: Removed dependency on `local_email`/`local_password` and the unauthenticated local dashboard fallback; remote mode is unchanged.
Handoff: Paired server PR #7; verified with `cargo test`; `cargo fmt --check` still has unrelated pre-existing diffs in `install_hooks.rs` and `status.rs`.
