use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
};

use chrono::Utc;
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    config::ConfigStore,
    error::Result,
    hooks::{CLAUDE_SOURCE, CODEX_SOURCE, span},
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
        Some(CLAUDE_SOURCE | CODEX_SOURCE | "opencode" | "openclaw") => source.unwrap(),
        _ => CLAUDE_SOURCE.to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActiveTurn {
    trace_id: String,
    span_id: String,
}

#[derive(Debug)]
struct ResolvedSpanContext {
    trace_id: String,
    span_id: String,
    parent_span_id: Option<String>,
}

fn timestamp_unix_nano() -> String {
    let now = Utc::now();
    now.timestamp_nanos_opt()
        .unwrap_or_else(|| now.timestamp_micros() * 1_000)
        .to_string()
}

fn stable_trace_id(parts: &[&str]) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, parts.join(":").as_bytes())
        .simple()
        .to_string()
}

fn stable_span_id(parts: &[&str]) -> String {
    stable_trace_id(parts).chars().take(16).collect()
}

fn random_trace_id() -> String {
    Uuid::new_v4().simple().to_string()
}

fn random_span_id() -> String {
    Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(16)
        .collect()
}

fn str_payload_field(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn event_identity(event_type: &str, payload: &Value) -> Option<String> {
    for key in [
        "event_id",
        "message_id",
        "prompt_id",
        "turn_id",
        "request_id",
        "tool_use_id",
        "agent_id",
        "call_id",
        "timestamp",
    ] {
        if let Some(value) = str_payload_field(payload, key) {
            return Some(format!("{key}:{value}"));
        }
    }

    if event_type == "user_prompt_submit" {
        return str_payload_field(payload, "prompt").map(|prompt| format!("prompt:{prompt}"));
    }

    None
}

fn active_turn_key(source: &str, session_id: &str) -> String {
    format!("{source}:{session_id}")
}

fn read_active_turns() -> BTreeMap<String, ActiveTurn> {
    let path = match ConfigStore::run_dir() {
        Ok(dir) => dir.join("active-turns.json"),
        Err(_) => return BTreeMap::new(),
    };
    let body = match fs::read_to_string(path) {
        Ok(body) => body,
        Err(_) => return BTreeMap::new(),
    };
    serde_json::from_str(&body).unwrap_or_default()
}

fn write_active_turns(turns: &BTreeMap<String, ActiveTurn>) {
    let run_dir = match ConfigStore::run_dir() {
        Ok(dir) => dir,
        Err(_) => return,
    };
    if fs::create_dir_all(&run_dir).is_err() {
        return;
    }
    let path = run_dir.join("active-turns.json");
    if let Ok(body) = serde_json::to_string(turns) {
        let _ = fs::write(path, body);
    }
}

fn resolve_span_context(
    event_type: &str,
    payload: &Value,
    project_id: &str,
    source: &str,
    session_id: &str,
) -> ResolvedSpanContext {
    let key = active_turn_key(source, session_id);

    if span::event_type_is_turn_start(event_type) {
        let identity = event_identity(event_type, payload).unwrap_or_else(random_trace_id);
        let trace_id =
            stable_trace_id(&["pulse", "turn", project_id, source, session_id, &identity]);
        let span_id = stable_span_id(&["pulse", "span", &trace_id, "agent.turn"]);
        let mut turns = read_active_turns();
        turns.insert(
            key,
            ActiveTurn {
                trace_id: trace_id.clone(),
                span_id: span_id.clone(),
            },
        );
        write_active_turns(&turns);
        return ResolvedSpanContext {
            trace_id,
            span_id,
            parent_span_id: None,
        };
    }

    if span::event_type_attaches_to_turn(event_type) {
        let mut turns = read_active_turns();
        if let Some(active) = turns.get(&key).cloned() {
            if event_type == "stop" {
                turns.remove(&key);
                write_active_turns(&turns);
            }
            let identity = event_identity(event_type, payload);
            let span_id = identity
                .as_deref()
                .map(|identity| {
                    stable_span_id(&["pulse", "span", &active.trace_id, event_type, identity])
                })
                .unwrap_or_else(random_span_id);
            return ResolvedSpanContext {
                trace_id: active.trace_id,
                span_id,
                parent_span_id: Some(active.span_id),
            };
        }
    }

    if matches!(event_type, "session_end") {
        let mut turns = read_active_turns();
        if turns.remove(&key).is_some() {
            write_active_turns(&turns);
        }
    }

    let trace_id = stable_trace_id(&["pulse", "session_lifecycle", project_id, source, session_id]);
    let span_id = event_identity(event_type, payload)
        .as_deref()
        .map(|identity| stable_span_id(&["pulse", "span", &trace_id, event_type, identity]))
        .unwrap_or_else(random_span_id);
    ResolvedSpanContext {
        trace_id,
        span_id,
        parent_span_id: None,
    }
}

async fn emit_inner(args: EmitArgs) -> Result<()> {
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

    emit_payload(&event_type, payload, None).await
}

pub async fn emit_payload(
    event_type: &str,
    mut payload: Value,
    source_override: Option<&str>,
) -> Result<()> {
    let config = match ConfigStore::load() {
        Ok(cfg) => cfg,
        Err(_) => return Ok(()),
    };

    if debug_enabled() {
        debug_log(event_type, &payload);
    }

    if let Some(source) = source_override {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("source".to_string(), Value::String(source.to_string()));
        }
    }

    let mut fields = span::extract(event_type, &payload);

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
    let Some(session_id) = fields.session_id.clone() else {
        return Ok(());
    };
    let span_context = resolve_span_context(
        event_type,
        &payload,
        &config.project_id,
        &source,
        &session_id,
    );

    let otlp_span = match fields.into_otlp_span(
        span_context.trace_id,
        span_context.span_id,
        span_context.parent_span_id,
        timestamp_unix_nano(),
        event_type.to_string(),
        source.clone(),
    ) {
        Some(s) => s,
        None => return Ok(()),
    };

    let client = match TraceHttpClient::new(&config) {
        Ok(client) => client,
        Err(_) => return Ok(()),
    };

    let resource_attributes = vec![
        crate::http::OtlpAttribute::string("service.name", "pulse-cli"),
        crate::http::OtlpAttribute::string("service.version", env!("CARGO_PKG_VERSION")),
        crate::http::OtlpAttribute::string("pulse.project.id", config.project_id),
        crate::http::OtlpAttribute::string("pulse.source", source),
    ];
    let traces = crate::http::OtlpTracePayload::single(otlp_span, resource_attributes);
    let _ = client.post_traces(&traces).await;

    Ok(())
}
