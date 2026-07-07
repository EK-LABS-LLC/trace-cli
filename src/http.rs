use std::time::Duration;

use reqwest::{Client, Url};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::{
    config::PulseConfig,
    error::{PulseError, Result},
};

const USER_AGENT: &str = concat!("pulse-cli/", env!("CARGO_PKG_VERSION"));
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const EMIT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct TraceHttpClient {
    client: Client,
    base_url: Url,
    api_key: String,
    project_id: String,
}

impl TraceHttpClient {
    pub fn new(config: &PulseConfig) -> Result<Self> {
        let base = normalize_base_url(&config.api_url)?;
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(DEFAULT_TIMEOUT)
            .build()?;

        Ok(Self {
            client,
            base_url: base,
            api_key: config.api_key.clone(),
            project_id: config.project_id.clone(),
        })
    }

    fn make_url(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path.trim_start_matches('/'))
            .map_err(|err| PulseError::message(format!("invalid url path: {err}")))
    }

    fn auth_headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("X-Project-Id", &self.project_id)
    }

    pub async fn health_check(&self) -> Result<()> {
        let url = self.make_url("/health")?;
        self.client.get(url).send().await?.error_for_status()?;
        Ok(())
    }

    pub async fn post_traces(&self, traces: &OtlpTracePayload) -> Result<()> {
        if traces.resource_spans.is_empty() {
            return Ok(());
        }
        let url = self.make_url("/v1/traces")?;
        self.auth_headers(self.client.post(url))
            .timeout(EMIT_TIMEOUT)
            .json(traces)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

fn normalize_base_url(raw: &str) -> Result<Url> {
    let trimmed = raw.trim().trim_end_matches('/');
    Url::parse(trimmed).map_err(|err| PulseError::message(format!("invalid API url: {err}")))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtlpTracePayload {
    pub resource_spans: Vec<ResourceSpans>,
}

impl OtlpTracePayload {
    pub fn single(span: OtlpSpan, resource_attributes: Vec<OtlpAttribute>) -> Self {
        Self {
            resource_spans: vec![ResourceSpans {
                resource: OtlpResource {
                    attributes: resource_attributes,
                },
                scope_spans: vec![ScopeSpans {
                    scope: OtlpScope {
                        name: "pulse.cli.hooks".to_string(),
                        version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    },
                    spans: vec![span],
                }],
            }],
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSpans {
    pub resource: OtlpResource,
    pub scope_spans: Vec<ScopeSpans>,
}

#[derive(Debug, Serialize)]
pub struct OtlpResource {
    pub attributes: Vec<OtlpAttribute>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeSpans {
    pub scope: OtlpScope,
    pub spans: Vec<OtlpSpan>,
}

#[derive(Debug, Serialize)]
pub struct OtlpScope {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtlpSpan {
    pub trace_id: String,
    pub span_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub start_time_unix_nano: String,
    pub end_time_unix_nano: String,
    pub attributes: Vec<OtlpAttribute>,
    pub status: OtlpStatus,
}

#[derive(Debug, Serialize)]
pub struct OtlpStatus {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OtlpAttribute {
    pub key: String,
    pub value: OtlpAnyValue,
}

impl OtlpAttribute {
    pub fn string(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: OtlpAnyValue {
                value: OtlpAnyValueKind::StringValue(value.into()),
            },
        }
    }

    pub fn bool(key: impl Into<String>, value: bool) -> Self {
        Self {
            key: key.into(),
            value: OtlpAnyValue {
                value: OtlpAnyValueKind::BoolValue(value),
            },
        }
    }

    pub fn json(key: impl Into<String>, value: Value) -> Self {
        Self {
            key: key.into(),
            value: value_to_any(value),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OtlpAnyValue {
    #[serde(flatten)]
    value: OtlpAnyValueKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
enum OtlpAnyValueKind {
    StringValue(String),
    BoolValue(bool),
    IntValue(String),
    DoubleValue(f64),
    ArrayValue(OtlpArrayValue),
    KvlistValue(OtlpKvListValue),
}

#[derive(Debug, Clone, Serialize)]
pub struct OtlpArrayValue {
    pub values: Vec<OtlpAnyValue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OtlpKvListValue {
    pub values: Vec<OtlpAttribute>,
}

fn value_to_any(value: Value) -> OtlpAnyValue {
    let value = match value {
        Value::Null => OtlpAnyValueKind::StringValue("null".to_string()),
        Value::Bool(value) => OtlpAnyValueKind::BoolValue(value),
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                OtlpAnyValueKind::IntValue(value.to_string())
            } else if let Some(value) = number.as_u64() {
                OtlpAnyValueKind::IntValue(value.to_string())
            } else {
                OtlpAnyValueKind::DoubleValue(number.as_f64().unwrap_or_default())
            }
        }
        Value::String(value) => OtlpAnyValueKind::StringValue(value),
        Value::Array(values) => OtlpAnyValueKind::ArrayValue(OtlpArrayValue {
            values: values.into_iter().map(value_to_any).collect(),
        }),
        Value::Object(values) => OtlpAnyValueKind::KvlistValue(OtlpKvListValue {
            values: map_to_attributes(values),
        }),
    };

    OtlpAnyValue { value }
}

fn map_to_attributes(values: Map<String, Value>) -> Vec<OtlpAttribute> {
    values
        .into_iter()
        .map(|(key, value)| OtlpAttribute::json(key, value))
        .collect()
}
