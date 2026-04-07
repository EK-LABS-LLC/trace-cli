use std::time::Duration;

use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Debug, Clone)]
pub struct SftExportQuery {
    pub session_id: Option<String>,
    pub source: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub max_sessions: u32,
    pub max_spans: u32,
    pub max_traces: u32,
    pub mode: String,
    pub format: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SftMessage {
    pub role: String,
    pub content: String,
    pub name: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<Vec<SftToolCall>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SftToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: SftToolFunction,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SftToolFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SftTrainingExample {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub messages: Vec<SftMessage>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SftExportResponse {
    pub examples: Vec<SftTrainingExample>,
    pub count: usize,
    pub mode: String,
    pub format: String,
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

    pub async fn post_spans(&self, spans: &[SpanPayload]) -> Result<()> {
        if spans.is_empty() {
            return Ok(());
        }
        let url = self.make_url("/v1/spans/async")?;
        self.auth_headers(self.client.post(url))
            .timeout(EMIT_TIMEOUT)
            .json(spans)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn export_sft_json(&self, query: &SftExportQuery) -> Result<SftExportResponse> {
        let mut url = self.make_url("/v1/training/sft")?;
        {
            let mut pairs = url.query_pairs_mut();
            if let Some(session_id) = &query.session_id {
                pairs.append_pair("session_id", session_id);
            }
            if let Some(source) = &query.source {
                pairs.append_pair("source", source);
            }
            if let Some(date_from) = &query.date_from {
                pairs.append_pair("date_from", date_from);
            }
            if let Some(date_to) = &query.date_to {
                pairs.append_pair("date_to", date_to);
            }
            pairs.append_pair("max_sessions", &query.max_sessions.to_string());
            pairs.append_pair("max_spans", &query.max_spans.to_string());
            pairs.append_pair("max_traces", &query.max_traces.to_string());
            pairs.append_pair("mode", &query.mode);
            pairs.append_pair("format", "json");
        }

        let response = self
            .auth_headers(self.client.get(url))
            .send()
            .await?
            .error_for_status()?;
        Ok(response.json::<SftExportResponse>().await?)
    }

    pub async fn export_sft_jsonl(&self, query: &SftExportQuery) -> Result<String> {
        let mut url = self.make_url("/v1/training/sft")?;
        {
            let mut pairs = url.query_pairs_mut();
            if let Some(session_id) = &query.session_id {
                pairs.append_pair("session_id", session_id);
            }
            if let Some(source) = &query.source {
                pairs.append_pair("source", source);
            }
            if let Some(date_from) = &query.date_from {
                pairs.append_pair("date_from", date_from);
            }
            if let Some(date_to) = &query.date_to {
                pairs.append_pair("date_to", date_to);
            }
            pairs.append_pair("max_sessions", &query.max_sessions.to_string());
            pairs.append_pair("max_spans", &query.max_spans.to_string());
            pairs.append_pair("max_traces", &query.max_traces.to_string());
            pairs.append_pair("mode", &query.mode);
            pairs.append_pair("format", "jsonl");
        }

        let response = self
            .auth_headers(self.client.get(url))
            .send()
            .await?
            .error_for_status()?;
        Ok(response.text().await?)
    }
}

fn normalize_base_url(raw: &str) -> Result<Url> {
    let trimmed = raw.trim().trim_end_matches('/');
    Url::parse(trimmed).map_err(|err| PulseError::message(format!("invalid API url: {err}")))
}

#[derive(Debug, Serialize)]
pub struct SpanPayload {
    pub span_id: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
    pub source: String,
    pub kind: String,
    pub event_type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_response: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_interrupt: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}
