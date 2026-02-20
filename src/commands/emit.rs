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

fn debug_enabled() -> bool {
    std::env::var("PULSE_DEBUG")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
}

fn debug_log(event_type: &str, payload: &Value) {
    use std::fs::OpenOptions;
    use std::io::Write;

    let path = std::env::var("PULSE_DEBUG_LOG").unwrap_or_else(|_| {
        dirs::home_dir()
            .map(|h| h.join(".pulse/debug.log").to_string_lossy().to_string())
            .unwrap_or_else(|| "/tmp/pulse-debug.log".to_string())
    });

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let ts = Utc::now().to_rfc3339();
        let pretty = serde_json::to_string_pretty(payload).unwrap_or_default();
        let _ = writeln!(file, "── [{ts}] {event_type} ──");
        let _ = writeln!(file, "{pretty}");
        let _ = writeln!(file);
    }
}

#[derive(Debug, Args)]
pub struct EmitArgs {
    /// Event type (e.g. post_tool_use, stop)
    pub event_type: String,
}

pub async fn run_emit(args: EmitArgs) {
    let _ = emit_inner(args).await;
}

fn normalized_source(source: Option<String>) -> String {
    match source.as_deref() {
        Some("claude_code" | "opencode" | "openclaw") => source.unwrap(),
        _ => CLAUDE_SOURCE.to_string(),
    }
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

    if debug_enabled() {
        debug_log(&event_type, &payload);
    }

    let mut fields = span::extract(&event_type, &payload);

    // Merge cli_version, project_id, and raw event payload into metadata.
    let meta = fields.metadata.get_or_insert_with(|| json!({}));
    if !meta.is_object() {
        *meta = json!({});
    }
    if let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "cli_version".to_string(),
            Value::String(env!("CARGO_PKG_VERSION").to_string()),
        );
        obj.insert(
            "project_id".to_string(),
            Value::String(config.project_id.clone()),
        );
        obj.insert("raw".to_string(), payload.clone());
    }

    let source = normalized_source(fields.source.take());

    let span = match fields.into_span(
        Uuid::new_v4().to_string(),
        Utc::now().to_rfc3339(),
        event_type,
        source.clone(),
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
