use pulse::http::{OtlpAttribute, OtlpSpan, OtlpStatus, OtlpTracePayload};
use serde_json::json;

fn minimal_span() -> OtlpSpan {
    OtlpSpan {
        trace_id: "550e8400e29b41d4a716446655440000".to_string(),
        span_id: "a716446655440000".to_string(),
        parent_span_id: None,
        name: "agent.tool".to_string(),
        kind: "SPAN_KIND_INTERNAL".to_string(),
        start_time_unix_nano: "1735689600000000000".to_string(),
        end_time_unix_nano: "1735689600000000000".to_string(),
        attributes: vec![
            OtlpAttribute::string("pulse.session.id", "sess_123"),
            OtlpAttribute::string("pulse.session.name", "sess_123"),
            OtlpAttribute::string("pulse.source", "claude_code"),
            OtlpAttribute::string("pulse.event.type", "post_tool_use"),
        ],
        status: OtlpStatus {
            code: "STATUS_CODE_OK".to_string(),
            message: None,
        },
    }
}

fn minimal_payload() -> OtlpTracePayload {
    OtlpTracePayload::single(
        minimal_span(),
        vec![
            OtlpAttribute::string("service.name", "pulse-cli"),
            OtlpAttribute::string("pulse.project.id", "project_123"),
        ],
    )
}

#[test]
fn serialization_uses_otlp_trace_shape() {
    let json = serde_json::to_value(minimal_payload()).unwrap();

    assert!(json["resourceSpans"].is_array());
    assert!(json["resourceSpans"][0]["scopeSpans"].is_array());
    assert!(json["resourceSpans"][0]["scopeSpans"][0]["spans"].is_array());
    assert_eq!(
        json["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["traceId"],
        "550e8400e29b41d4a716446655440000"
    );
    assert_eq!(
        json["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["spanId"],
        "a716446655440000"
    );
}

#[test]
fn serialization_includes_pulse_attributes() {
    let json = serde_json::to_value(minimal_payload()).unwrap();
    let attrs = json["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]
        .as_array()
        .unwrap();
    let attr = |key: &str| {
        attrs
            .iter()
            .find(|attr| attr["key"] == key)
            .expect("attribute should exist")
    };

    assert_eq!(attr("pulse.session.id")["value"]["stringValue"], "sess_123");
    assert_eq!(
        attr("pulse.session.name")["value"]["stringValue"],
        "sess_123"
    );
    assert_eq!(attr("pulse.source")["value"]["stringValue"], "claude_code");
    assert_eq!(
        attr("pulse.event.type")["value"]["stringValue"],
        "post_tool_use"
    );
}

#[test]
fn serialization_omits_absent_parent_and_status_message() {
    let json = serde_json::to_value(minimal_payload()).unwrap();
    let span = json["resourceSpans"][0]["scopeSpans"][0]["spans"][0]
        .as_object()
        .unwrap();

    assert!(!span.contains_key("parentSpanId"));
    assert!(!span["status"].as_object().unwrap().contains_key("message"));
}

#[test]
fn serialization_includes_parent_when_set() {
    let mut span = minimal_span();
    span.parent_span_id = Some("0011223344556677".to_string());
    let json = serde_json::to_value(OtlpTracePayload::single(span, Vec::new())).unwrap();

    assert_eq!(
        json["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["parentSpanId"],
        "0011223344556677"
    );
}

#[test]
fn serialization_converts_json_attributes_to_otlp_any_values() {
    let mut span = minimal_span();
    span.attributes.push(OtlpAttribute::json(
        "pulse.metadata",
        json!({
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cost": 0.0042
            },
            "cached": true
        }),
    ));

    let json = serde_json::to_value(OtlpTracePayload::single(span, Vec::new())).unwrap();
    let attrs = json["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]
        .as_array()
        .unwrap();
    let metadata = attrs
        .iter()
        .find(|attr| attr["key"] == "pulse.metadata")
        .unwrap();
    let values = metadata["value"]["kvlistValue"]["values"]
        .as_array()
        .unwrap();

    assert!(values.iter().any(|attr| attr["key"] == "usage"));
    assert!(values.iter().any(|attr| attr["key"] == "cached"));
}
