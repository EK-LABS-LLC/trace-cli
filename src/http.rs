use std::time::Duration;

use reqwest::{Client, Url};
use serde::Serialize;
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

    pub async fn post_events(&self, events: &[EventPayload]) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let url = self.make_url("/v1/events/batch")?;
        self.auth_headers(self.client.post(url))
            .timeout(EMIT_TIMEOUT)
            .json(events)
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
pub struct EventPayload {
    pub session_id: String,
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}
