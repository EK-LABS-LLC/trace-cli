use pulse::hooks::span;
use serde_json::json;

#[test]
fn event_type_to_kind_mappings() {
    assert_eq!(span::event_type_to_kind("pre_tool_use"), "tool_use");
    assert_eq!(span::event_type_to_kind("post_tool_use"), "tool_use");
    assert_eq!(
        span::event_type_to_kind("post_tool_use_failure"),
        "tool_use"
    );
    assert_eq!(span::event_type_to_kind("session_start"), "session");
    assert_eq!(span::event_type_to_kind("session_end"), "session");
    assert_eq!(span::event_type_to_kind("stop"), "session");
    assert_eq!(span::event_type_to_kind("subagent_start"), "agent_run");
    assert_eq!(span::event_type_to_kind("subagent_stop"), "agent_run");
    assert_eq!(
        span::event_type_to_kind("user_prompt_submit"),
        "user_prompt"
    );
    assert_eq!(
        span::event_type_to_kind("assistant_message"),
        "llm_response"
    );
    assert_eq!(span::event_type_to_kind("notification"), "notification");
    assert_eq!(span::event_type_to_kind("unknown_event"), "session");
}

#[test]
fn event_type_to_status_mappings() {
    assert_eq!(span::event_type_to_status("post_tool_use_failure"), "error");
    assert_eq!(span::event_type_to_status("post_tool_use"), "success");
    assert_eq!(span::event_type_to_status("session_start"), "success");
    assert_eq!(span::event_type_to_status("stop"), "success");
    assert_eq!(span::event_type_to_status("assistant_message"), "success");
}

#[test]
fn extract_common_fields() {
    let payload = json!({
        "session_id": "sess_123",
        "cwd": "/home/user/project"
    });
    let fields = span::extract("stop", &payload);
    assert_eq!(fields.session_id.as_deref(), Some("sess_123"));
    assert_eq!(fields.cwd.as_deref(), Some("/home/user/project"));
}

#[test]
fn extract_common_ignores_empty_strings() {
    let payload = json!({
        "session_id": "",
        "cwd": ""
    });
    let fields = span::extract("stop", &payload);
    assert!(fields.session_id.is_none());
    assert!(fields.cwd.is_none());
}

#[test]
fn extract_pre_tool_use() {
    let payload = json!({
        "session_id": "sess_1",
        "tool_use_id": "tu_abc",
        "tool_name": "Bash",
        "tool_input": {"command": "ls -la"}
    });
    let fields = span::extract("pre_tool_use", &payload);
    assert_eq!(fields.tool_use_id.as_deref(), Some("tu_abc"));
    assert_eq!(fields.tool_name.as_deref(), Some("Bash"));
    assert_eq!(fields.tool_input, Some(json!({"command": "ls -la"})));
    assert!(fields.tool_response.is_none());
}

#[test]
fn extract_post_tool_use() {
    let payload = json!({
        "session_id": "sess_1",
        "tool_use_id": "tu_abc",
        "tool_name": "Read",
        "tool_input": {"file": "foo.rs"},
        "tool_response": "file contents here"
    });
    let fields = span::extract("post_tool_use", &payload);
    assert_eq!(fields.tool_use_id.as_deref(), Some("tu_abc"));
    assert_eq!(fields.tool_name.as_deref(), Some("Read"));
    assert_eq!(fields.tool_input, Some(json!({"file": "foo.rs"})));
    assert_eq!(fields.tool_response, Some(json!("file contents here")));
}

#[test]
fn extract_post_tool_use_failure() {
    let payload = json!({
        "session_id": "sess_1",
        "tool_use_id": "tu_abc",
        "tool_name": "Bash",
        "tool_input": {"command": "rm -rf /"},
        "error": "permission denied",
        "is_interrupt": true
    });
    let fields = span::extract("post_tool_use_failure", &payload);
    assert_eq!(fields.tool_name.as_deref(), Some("Bash"));
    assert_eq!(fields.error, Some(json!("permission denied")));
    assert_eq!(fields.is_interrupt, Some(true));
}

#[test]
fn extract_session_start() {
    let payload = json!({
        "session_id": "sess_1",
        "model": "claude-sonnet-4-20250514"
    });
    let fields = span::extract("session_start", &payload);
    assert_eq!(fields.model.as_deref(), Some("claude-sonnet-4-20250514"));
}

#[test]
fn extract_session_end() {
    let payload = json!({
        "session_id": "sess_1",
        "reason": "user_exit"
    });
    let fields = span::extract("session_end", &payload);
    let meta = fields.metadata.unwrap();
    assert_eq!(meta["reason"], "user_exit");
}

#[test]
fn extract_subagent_start() {
    let payload = json!({
        "session_id": "sess_1",
        "agent_type": "code_reviewer",
        "agent_id": "agent_xyz"
    });
    let fields = span::extract("subagent_start", &payload);
    assert_eq!(fields.agent_name.as_deref(), Some("code_reviewer"));
    let meta = fields.metadata.unwrap();
    assert_eq!(meta["agent_id"], "agent_xyz");
}

#[test]
fn extract_subagent_falls_back_to_agent_name() {
    let payload = json!({
        "session_id": "sess_1",
        "agent_name": "fallback_agent"
    });
    let fields = span::extract("subagent_stop", &payload);
    assert_eq!(fields.agent_name.as_deref(), Some("fallback_agent"));
}

#[test]
fn extract_user_prompt() {
    let payload = json!({
        "session_id": "sess_1",
        "prompt": "fix the bug in main.rs"
    });
    let fields = span::extract("user_prompt_submit", &payload);
    let meta = fields.metadata.unwrap();
    assert_eq!(meta["prompt"], "fix the bug in main.rs");
}

#[test]
fn extract_notification() {
    let payload = json!({
        "session_id": "sess_1",
        "message": "Build succeeded",
        "title": "CI"
    });
    let fields = span::extract("notification", &payload);
    let meta = fields.metadata.unwrap();
    assert_eq!(meta["message"], "Build succeeded");
    assert_eq!(meta["title"], "CI");
}

#[test]
fn extract_assistant_message() {
    let payload = json!({
        "session_id": "sess_1",
        "model": "claude-sonnet-4-20250514",
        "tokens": {
            "input": 100,
            "output": 50,
            "reasoning": 10,
            "cache": { "read": 5, "write": 3 }
        },
        "cost": 0.0042
    });
    let fields = span::extract("assistant_message", &payload);
    assert_eq!(fields.model.as_deref(), Some("claude-sonnet-4-20250514"));
    let usage = &fields.metadata.as_ref().unwrap()["usage"];
    assert_eq!(usage["input_tokens"], 100);
    assert_eq!(usage["output_tokens"], 50);
    assert_eq!(usage["reasoning_tokens"], 10);
    assert_eq!(usage["cache_read_tokens"], 5);
    assert_eq!(usage["cache_write_tokens"], 3);
    assert_eq!(usage["cost"], 0.0042);
}

#[test]
fn extract_assistant_message_partial_tokens() {
    let payload = json!({
        "session_id": "sess_1",
        "tokens": { "input": 100, "output": 50 },
        "cost": 0.001
    });
    let fields = span::extract("assistant_message", &payload);
    let usage = &fields.metadata.as_ref().unwrap()["usage"];
    assert_eq!(usage["input_tokens"], 100);
    assert_eq!(usage["output_tokens"], 50);
    assert!(usage.get("reasoning_tokens").is_none());
    assert!(usage.get("cache_read_tokens").is_none());
    assert!(usage.get("cache_write_tokens").is_none());
    assert_eq!(usage["cost"], 0.001);
}

#[test]
fn extract_assistant_message_no_tokens() {
    let payload = json!({
        "session_id": "sess_1"
    });
    let fields = span::extract("assistant_message", &payload);
    assert!(fields.metadata.is_none());
}

#[test]
fn extract_unknown_event_type() {
    let payload = json!({
        "session_id": "sess_1",
        "some_field": "value"
    });
    let fields = span::extract("totally_unknown", &payload);
    assert_eq!(fields.session_id.as_deref(), Some("sess_1"));
    assert!(fields.tool_name.is_none());
}

#[test]
fn into_span_returns_none_without_session_id() {
    let payload = json!({"tool_name": "Bash"});
    let fields = span::extract("post_tool_use", &payload);
    let span = fields.into_span(
        "span-id".to_string(),
        "2025-01-01T00:00:00Z".to_string(),
        "post_tool_use".to_string(),
        "claude_code".to_string(),
    );
    assert!(span.is_none());
}

#[test]
fn into_span_builds_correct_payload() {
    let payload = json!({
        "session_id": "sess_1",
        "tool_use_id": "tu_1",
        "tool_name": "Bash",
        "tool_input": {"command": "ls"},
        "cwd": "/tmp"
    });
    let fields = span::extract("post_tool_use", &payload);
    let span = fields
        .into_span(
            "span-id-123".to_string(),
            "2025-01-01T00:00:00Z".to_string(),
            "post_tool_use".to_string(),
            "claude_code".to_string(),
        )
        .unwrap();

    assert_eq!(span.span_id, "span-id-123");
    assert_eq!(span.session_id, "sess_1");
    assert_eq!(span.event_type, "post_tool_use");
    assert_eq!(span.kind, "tool_use");
    assert_eq!(span.status, "success");
    assert_eq!(span.source, "claude_code");
    assert_eq!(span.tool_name.as_deref(), Some("Bash"));
    assert_eq!(span.cwd.as_deref(), Some("/tmp"));
}
