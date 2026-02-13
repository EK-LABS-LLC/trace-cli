use std::io::{self, Read};

use chrono::Utc;
use clap::Args;
use serde_json::{Value, json};

use crate::{
    config::ConfigStore,
    error::Result,
    hooks::CLAUDE_SOURCE,
    http::{EventPayload, TraceHttpClient},
};

const DEFAULT_SOURCE: &str = CLAUDE_SOURCE;

#[derive(Debug, Args)]
pub struct EmitArgs {
    /// Event type (e.g. post_tool_use, stop)
    pub event_type: String,
    /// Override the event source label (defaults to claude_code)
    #[arg(long)]
    pub source: Option<String>,
}

pub async fn run_emit(args: EmitArgs) {
    let _ = emit_inner(args).await;
}

async fn emit_inner(args: EmitArgs) -> Result<()> {
    let EmitArgs { event_type, source } = args;

    let config = match ConfigStore::load() {
        Ok(cfg) => cfg,
        Err(_) => return Ok(()),
    };

    let event_type = event_type.trim().to_string();
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

    let session_id = payload
        .get("session_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());

    let Some(session_id) = session_id else {
        return Ok(());
    };

    let tool_name = payload
        .get("tool_name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());

    let source = source
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .or_else(|| {
            payload
                .get("source")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| DEFAULT_SOURCE.to_string());

    let client = match TraceHttpClient::new(&config) {
        Ok(client) => client,
        Err(_) => return Ok(()),
    };

    let event = EventPayload {
        session_id,
        event_type,
        tool_name,
        timestamp: Utc::now().to_rfc3339(),
        payload: Some(payload),
        source,
        metadata: Some(json!({
            "cli_version": env!("CARGO_PKG_VERSION"),
            "project_id": config.project_id.clone(),
        })),
    };

    let _ = client.post_events(&[event]).await;

    Ok(())
}
