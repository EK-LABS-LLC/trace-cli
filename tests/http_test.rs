use pulse::http::SpanPayload;
use serde_json::json;

fn minimal_span() -> SpanPayload {
    SpanPayload {
        span_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
        session_id: "sess_123".to_string(),
        parent_span_id: None,
        timestamp: "2025-01-01T00:00:00+00:00".to_string(),
        duration_ms: None,
        source: "claude_code".to_string(),
        kind: "tool_use".to_string(),
        event_type: "post_tool_use".to_string(),
        status: "success".to_string(),
        tool_use_id: None,
        tool_name: None,
        tool_input: None,
        tool_response: None,
        error: None,
        is_interrupt: None,
        cwd: None,
        model: None,
        agent_name: None,
        metadata: None,
    }
}

#[test]
fn serialization_includes_required_fields() {
    let span = minimal_span();
    let json = serde_json::to_value(&span).unwrap();

    assert_eq!(json["span_id"], "550e8400-e29b-41d4-a716-446655440000");
    assert_eq!(json["session_id"], "sess_123");
    assert_eq!(json["source"], "claude_code");
    assert_eq!(json["kind"], "tool_use");
    assert_eq!(json["event_type"], "post_tool_use");
    assert_eq!(json["status"], "success");
}

#[test]
fn serialization_omits_none_fields() {
    let span = minimal_span();
    let json = serde_json::to_value(&span).unwrap();
    let obj = json.as_object().unwrap();

    assert!(!obj.contains_key("parent_span_id"));
    assert!(!obj.contains_key("duration_ms"));
    assert!(!obj.contains_key("tool_use_id"));
    assert!(!obj.contains_key("tool_name"));
    assert!(!obj.contains_key("tool_input"));
    assert!(!obj.contains_key("tool_response"));
    assert!(!obj.contains_key("error"));
    assert!(!obj.contains_key("is_interrupt"));
    assert!(!obj.contains_key("cwd"));
    assert!(!obj.contains_key("model"));
    assert!(!obj.contains_key("agent_name"));
    assert!(!obj.contains_key("metadata"));
}

#[test]
fn serialization_includes_optional_fields_when_set() {
    let mut span = minimal_span();
    span.tool_use_id = Some("tu_abc".to_string());
    span.tool_name = Some("Bash".to_string());
    span.tool_input = Some(json!({"command": "ls"}));
    span.cwd = Some("/tmp".to_string());
    span.metadata = Some(json!({"cli_version": "0.1.0"}));

    let json = serde_json::to_value(&span).unwrap();
    assert_eq!(json["tool_use_id"], "tu_abc");
    assert_eq!(json["tool_name"], "Bash");
    assert_eq!(json["tool_input"]["command"], "ls");
    assert_eq!(json["cwd"], "/tmp");
    assert_eq!(json["metadata"]["cli_version"], "0.1.0");
}

#[test]
fn serialization_includes_usage_in_metadata() {
    let mut span = minimal_span();
    span.metadata = Some(json!({
        "usage": {
            "input_tokens": 100,
            "output_tokens": 50,
            "reasoning_tokens": 10,
            "cache_read_tokens": 5,
            "cache_write_tokens": 3,
            "cost": 0.0042
        }
    }));

    let json = serde_json::to_value(&span).unwrap();
    let usage = &json["metadata"]["usage"];
    assert_eq!(usage["input_tokens"], 100);
    assert_eq!(usage["output_tokens"], 50);
    assert_eq!(usage["reasoning_tokens"], 10);
    assert_eq!(usage["cache_read_tokens"], 5);
    assert_eq!(usage["cache_write_tokens"], 3);
    assert_eq!(usage["cost"], 0.0042);
}

#[test]
fn serialization_batch_format() {
    let spans = vec![minimal_span(), minimal_span()];
    let json = serde_json::to_value(&spans).unwrap();
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 2);
}
