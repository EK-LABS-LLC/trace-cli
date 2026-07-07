use serde_json::Value;

use crate::http::{OtlpAttribute, OtlpSpan, OtlpStatus};

pub struct SpanFields {
    pub session_id: Option<String>,
    pub session_name: Option<String>,
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
            session_name: None,
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

    pub fn into_otlp_span(
        self,
        trace_id: String,
        span_id: String,
        parent_span_id: Option<String>,
        timestamp_unix_nano: String,
        event_type: String,
        source: String,
    ) -> Option<OtlpSpan> {
        let session_id = self.session_id.clone()?;
        let status = event_type_to_status(&event_type);
        Some(OtlpSpan {
            trace_id,
            span_id,
            parent_span_id,
            name: event_type_to_span_name(&event_type).to_string(),
            kind: "SPAN_KIND_INTERNAL".to_string(),
            start_time_unix_nano: timestamp_unix_nano.clone(),
            end_time_unix_nano: timestamp_unix_nano,
            attributes: self.into_attributes(&session_id, &event_type, &source),
            status: OtlpStatus {
                code: if status == "error" {
                    "STATUS_CODE_ERROR".to_string()
                } else {
                    "STATUS_CODE_OK".to_string()
                },
                message: None,
            },
        })
    }

    fn into_attributes(
        self,
        session_id: &str,
        event_type: &str,
        source: &str,
    ) -> Vec<OtlpAttribute> {
        let mut attrs = vec![
            OtlpAttribute::string("pulse.session.id", session_id),
            OtlpAttribute::string(
                "pulse.session.name",
                self.session_name.unwrap_or_else(|| session_id.to_string()),
            ),
            OtlpAttribute::string("pulse.source", source),
            OtlpAttribute::string("pulse.event.type", event_type),
            OtlpAttribute::string("pulse.event.kind", event_type_to_kind(event_type)),
            OtlpAttribute::string("pulse.event.status", event_type_to_status(event_type)),
        ];

        push_string_attr(&mut attrs, "pulse.cwd", self.cwd);
        push_string_attr(&mut attrs, "pulse.model", self.model);
        push_string_attr(&mut attrs, "pulse.tool.id", self.tool_use_id);
        push_string_attr(&mut attrs, "pulse.tool.name", self.tool_name);
        push_json_attr(&mut attrs, "pulse.tool.input", self.tool_input);
        push_json_attr(&mut attrs, "pulse.tool.response", self.tool_response);
        push_json_attr(&mut attrs, "pulse.error", self.error);
        if let Some(is_interrupt) = self.is_interrupt {
            attrs.push(OtlpAttribute::bool("pulse.is_interrupt", is_interrupt));
        }
        push_string_attr(&mut attrs, "pulse.agent.name", self.agent_name);
        push_json_attr(&mut attrs, "pulse.metadata", self.metadata);

        attrs
    }
}

pub fn extract(event_type: &str, payload: &Value) -> SpanFields {
    let mut fields = extract_common(payload);

    match event_type {
        "pre_tool_use" | "permission_request" => extract_pre_tool_use(payload, &mut fields),
        "post_tool_use" => extract_post_tool_use(payload, &mut fields),
        "post_tool_use_failure" => extract_post_tool_use_failure(payload, &mut fields),
        "session_start" => extract_session_start(payload, &mut fields),
        "session_end" => extract_session_end(payload, &mut fields),
        "stop" => {}
        "subagent_start" => extract_subagent(payload, &mut fields),
        "subagent_stop" => extract_subagent(payload, &mut fields),
        "user_prompt_submit" => extract_user_prompt(payload, &mut fields),
        "assistant_message" => extract_assistant_message(payload, &mut fields),
        "notification" => extract_notification(payload, &mut fields),
        _ => {}
    }

    fields
}

pub fn event_type_to_kind(event_type: &str) -> &str {
    match event_type {
        "pre_tool_use" | "post_tool_use" | "post_tool_use_failure" => "tool_use",
        "permission_request" => "tool_use",
        "session_start" | "session_end" | "stop" => "session",
        "subagent_start" | "subagent_stop" => "agent_run",
        "user_prompt_submit" => "user_prompt",
        "assistant_message" => "llm_response",
        "notification" => "notification",
        _ => "session",
    }
}

pub fn event_type_to_span_name(event_type: &str) -> &str {
    match event_type {
        "user_prompt_submit" => "agent.turn",
        "pre_tool_use" | "post_tool_use" | "post_tool_use_failure" | "permission_request" => {
            "agent.tool"
        }
        "assistant_message" => "agent.assistant",
        "subagent_start" | "subagent_stop" => "agent.subagent",
        "stop" => "agent.stop",
        _ => "agent.session_lifecycle",
    }
}

pub fn event_type_is_turn_start(event_type: &str) -> bool {
    event_type == "user_prompt_submit"
}

pub fn event_type_attaches_to_turn(event_type: &str) -> bool {
    matches!(
        event_type,
        "assistant_message"
            | "pre_tool_use"
            | "post_tool_use"
            | "post_tool_use_failure"
            | "permission_request"
            | "subagent_start"
            | "subagent_stop"
            | "stop"
    )
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
    fields.session_name = str_field(payload, "session_name");
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
        let meta = fields.metadata.get_or_insert_with(|| serde_json::json!({}));
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

fn extract_assistant_message(payload: &Value, fields: &mut SpanFields) {
    let mut usage = serde_json::Map::new();

    if let Some(tokens) = payload.get("tokens") {
        if let Some(v) = tokens.get("input").and_then(|v| v.as_u64()) {
            usage.insert("input_tokens".to_string(), Value::Number(v.into()));
        }
        if let Some(v) = tokens.get("output").and_then(|v| v.as_u64()) {
            usage.insert("output_tokens".to_string(), Value::Number(v.into()));
        }
        if let Some(v) = tokens.get("reasoning").and_then(|v| v.as_u64()) {
            usage.insert("reasoning_tokens".to_string(), Value::Number(v.into()));
        }
        if let Some(cache) = tokens.get("cache") {
            if let Some(v) = cache.get("read").and_then(|v| v.as_u64()) {
                usage.insert("cache_read_tokens".to_string(), Value::Number(v.into()));
            }
            if let Some(v) = cache.get("write").and_then(|v| v.as_u64()) {
                usage.insert("cache_write_tokens".to_string(), Value::Number(v.into()));
            }
        }
    }

    if let Some(cost) = payload.get("cost").and_then(|v| v.as_f64()) {
        if let Some(n) = serde_json::Number::from_f64(cost) {
            usage.insert("cost".to_string(), Value::Number(n));
        }
    }

    if !usage.is_empty() {
        let meta = fields.metadata.get_or_insert_with(|| serde_json::json!({}));
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("usage".to_string(), Value::Object(usage));
        }
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

fn push_string_attr(attrs: &mut Vec<OtlpAttribute>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        attrs.push(OtlpAttribute::string(key, value));
    }
}

fn push_json_attr(attrs: &mut Vec<OtlpAttribute>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        attrs.push(OtlpAttribute::json(key, value));
    }
}
