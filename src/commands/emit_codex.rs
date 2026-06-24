use std::io::{self, Read};

use clap::Args;
use serde_json::Value;

use crate::{error::Result, hooks::CODEX_SOURCE};

use super::emit::emit_payload;

#[derive(Debug, Args)]
pub struct EmitCodexArgs {
    /// Normalized event type emitted by the Codex hook.
    pub event_type: String,
}

pub async fn run_emit_codex(args: EmitCodexArgs) {
    let _ = emit_codex_inner(args).await;
}

async fn emit_codex_inner(args: EmitCodexArgs) -> Result<()> {
    let event_type = args.event_type.trim().to_string();
    if event_type.is_empty() {
        return Ok(());
    }

    let mut stdin = String::new();
    if io::stdin().read_to_string(&mut stdin).is_err() {
        return Ok(());
    }

    if stdin.trim().is_empty() {
        return Ok(());
    }

    let payload: Value = match serde_json::from_str(&stdin) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };

    emit_payload(&event_type, payload, Some(CODEX_SOURCE)).await
}
