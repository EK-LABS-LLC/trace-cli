use serde_json::Value;

use crate::http::SpanPayload;

pub struct SpanFields {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub tool_use_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<Value>,
    pub tool_response: Option<Value>,
    pub error: Option<Value>,
    pub is_interrupt: Option<bool>,
    pub model: Option<String>,
    pub agent_name: Option<String>,
    pub metadata: Option<Value>,
    pub source: Option<String>,
}

impl SpanFields {
    fn new() -> Self {
        Self {
            session_id: None,
            cwd: None,
            tool_use_id: None,
            tool_name: None,
            tool_input: None,
            tool_response: None,
            error: None,
            is_interrupt: None,
            model: None,
            agent_name: None,
            metadata: None,
            source: None,
        }
    }

    pub fn into_span(
        self,
        span_id: String,
        timestamp: String,
        event_type: String,
        source: String,
    ) -> Option<SpanPayload> {
        let session_id = self.session_id?;
        Some(SpanPayload {
            span_id,
            session_id,
            parent_span_id: None,
            timestamp,
            duration_ms: None,
            source,
            kind: event_type_to_kind(&event_type).to_string(),
            status: event_type_to_status(&event_type).to_string(),
            event_type,
            tool_use_id: self.tool_use_id,
            tool_name: self.tool_name,
            tool_input: self.tool_input,
            tool_response: self.tool_response,
            error: self.error,
            is_interrupt: self.is_interrupt,
            cwd: self.cwd,
            model: self.model,
            agent_name: self.agent_name,
            metadata: self.metadata,
        })
    }
}

pub fn extract(event_type: &str, payload: &Value) -> SpanFields {
    let mut fields = extract_common(payload);

    match event_type {
        "pre_tool_use" => extract_pre_tool_use(payload, &mut fields),
        "post_tool_use" => extract_post_tool_use(payload, &mut fields),
        "post_tool_use_failure" => extract_post_tool_use_failure(payload, &mut fields),
        "session_start" => extract_session_start(payload, &mut fields),
        "session_end" => extract_session_end(payload, &mut fields),
        "stop" => {}
        "subagent_start" => extract_subagent(payload, &mut fields),
        "subagent_stop" => extract_subagent(payload, &mut fields),
        "user_prompt_submit" => extract_user_prompt(payload, &mut fields),
        "notification" => extract_notification(payload, &mut fields),
        _ => {}
    }

    fields
}

pub fn event_type_to_kind(event_type: &str) -> &str {
    match event_type {
        "pre_tool_use" | "post_tool_use" | "post_tool_use_failure" => "tool_use",
        "session_start" | "session_end" | "stop" => "session",
        "subagent_start" | "subagent_stop" => "agent_run",
        "user_prompt_submit" => "user_prompt",
        "notification" => "notification",
        _ => "session",
    }
}

pub fn event_type_to_status(event_type: &str) -> &str {
    match event_type {
        "post_tool_use_failure" => "error",
        _ => "success",
    }
}

fn str_field(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn extract_common(payload: &Value) -> SpanFields {
    let mut fields = SpanFields::new();
    fields.session_id = str_field(payload, "session_id");
    fields.cwd = str_field(payload, "cwd");
    fields.model = str_field(payload, "model");
    fields.source = str_field(payload, "source");
    fields
}

fn extract_tool_common(payload: &Value, fields: &mut SpanFields) {
    fields.tool_use_id = str_field(payload, "tool_use_id");
    fields.tool_name = str_field(payload, "tool_name");
    if let Some(input) = payload.get("tool_input").cloned() {
        fields.tool_input = Some(input);
    }
}

fn extract_pre_tool_use(payload: &Value, fields: &mut SpanFields) {
    extract_tool_common(payload, fields);
}

fn extract_post_tool_use(payload: &Value, fields: &mut SpanFields) {
    extract_tool_common(payload, fields);
    if let Some(response) = payload.get("tool_response").cloned() {
        fields.tool_response = Some(response);
    }
}

fn extract_post_tool_use_failure(payload: &Value, fields: &mut SpanFields) {
    extract_tool_common(payload, fields);
    if let Some(error) = payload.get("error").cloned() {
        fields.error = Some(error);
    }
    if let Some(is_interrupt) = payload.get("is_interrupt").and_then(|v| v.as_bool()) {
        fields.is_interrupt = Some(is_interrupt);
    }
}

fn extract_session_start(payload: &Value, fields: &mut SpanFields) {
    fields.model = str_field(payload, "model");
}

fn extract_session_end(payload: &Value, fields: &mut SpanFields) {
    if let Some(reason) = str_field(payload, "reason") {
        fields.metadata = Some(serde_json::json!({ "reason": reason }));
    }
}

fn extract_subagent(payload: &Value, fields: &mut SpanFields) {
    fields.agent_name = str_field(payload, "agent_type");
    if fields.agent_name.is_none() {
        fields.agent_name = str_field(payload, "agent_name");
    }
    if let Some(id) = str_field(payload, "agent_id") {
        let meta = fields
            .metadata
            .get_or_insert_with(|| serde_json::json!({}));
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("agent_id".to_string(), Value::String(id));
        }
    }
}

fn extract_user_prompt(payload: &Value, fields: &mut SpanFields) {
    if let Some(prompt) = str_field(payload, "prompt") {
        fields.metadata = Some(serde_json::json!({ "prompt": prompt }));
    }
}

fn extract_notification(payload: &Value, fields: &mut SpanFields) {
    let mut meta = serde_json::Map::new();
    if let Some(message) = str_field(payload, "message") {
        meta.insert("message".to_string(), Value::String(message));
    }
    if let Some(title) = str_field(payload, "title") {
        meta.insert("title".to_string(), Value::String(title));
    }
    if !meta.is_empty() {
        fields.metadata = Some(Value::Object(meta));
    }
}
