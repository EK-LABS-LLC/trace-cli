use std::io::{self, Read};

use chrono::Utc;
use clap::Args;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    config::ConfigStore,
    error::Result,
    hooks::{CLAUDE_SOURCE, span},
    http::TraceHttpClient,
};

#[derive(Debug, Args)]
pub struct EmitArgs {
    /// Event type (e.g. post_tool_use, stop)
    pub event_type: String,
}

pub async fn run_emit(args: EmitArgs) {
    let _ = emit_inner(args).await;
}

async fn emit_inner(args: EmitArgs) -> Result<()> {
    let event_type = args.event_type.trim().to_string();
    if event_type.is_empty() {
        return Ok(());
    }

    let config = match ConfigStore::load() {
        Ok(cfg) => cfg,
        Err(_) => return Ok(()),
    };

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

    let mut fields = span::extract(&event_type, &payload);

    // Merge cli_version and project_id into metadata
    let meta = fields
        .metadata
        .get_or_insert_with(|| json!({}));
    if let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "cli_version".to_string(),
            Value::String(env!("CARGO_PKG_VERSION").to_string()),
        );
        obj.insert(
            "project_id".to_string(),
            Value::String(config.project_id.clone()),
        );
    }

    let span = match fields.into_span(
        Uuid::new_v4().to_string(),
        Utc::now().to_rfc3339(),
        event_type,
        CLAUDE_SOURCE.to_string(),
    ) {
        Some(s) => s,
        None => return Ok(()),
    };

    let client = match TraceHttpClient::new(&config) {
        Ok(client) => client,
        Err(_) => return Ok(()),
    };

    let _ = client.post_spans(&[span]).await;

    Ok(())
}
