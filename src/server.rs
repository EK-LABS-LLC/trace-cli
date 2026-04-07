use std::process::{Command, Stdio};
use std::time::Duration;

use reqwest::{Client, Url};
use tokio::time::sleep;
use uuid::Uuid;

use crate::config::PulseConfig;
use crate::error::{PulseError, Result};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_INTERVAL: Duration = Duration::from_millis(500);
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const USER_AGENT: &str = concat!("pulse-cli/", env!("CARGO_PKG_VERSION"));

/// Ensure the trace service is running. If the server is unreachable and the
/// config has a `server_dir`, spawn `bun run pulse.ts` with the necessary env
/// vars and wait until healthy.
pub async fn ensure_server(config: &PulseConfig) -> Result<()> {
    let base_url = normalize_base_url(&config.api_url)?;
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(HTTP_TIMEOUT)
        .build()?;

    if is_healthy(&client, &base_url).await {
        return Ok(());
    }

    let server_dir = config.server_dir.as_deref().ok_or_else(|| {
        PulseError::message(
            "Trace service is not reachable and no server_dir is configured.\n\
             Start the server manually or run `pulse setup --local` to configure auto-start.",
        )
    })?;

    if !is_local_host(&base_url) {
        return Err(PulseError::message(format!(
            "Trace service is not reachable at {} and this is not a local URL.\n\
             Start your remote service manually.",
            base_url
        )));
    }

    eprintln!("Trace service not running. Starting server from {}...", server_dir);

    let mut command = Command::new("bun");
    command
        .arg("run")
        .arg("pulse.ts")
        .current_dir(server_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    apply_server_env(config, &mut command, &base_url);

    let child = command.spawn().map_err(|err| {
        PulseError::message(format!(
            "Failed to start server from {server_dir}: {err}\n\
             Make sure `bun` is installed and {server_dir}/pulse.ts exists.",
        ))
    })?;

    eprintln!("Started server (pid={}).", child.id());

    if wait_until_healthy(&client, &base_url, HEALTH_TIMEOUT, HEALTH_INTERVAL).await {
        eprintln!("Trace service is ready at {}", base_url);
        return Ok(());
    }

    Err(PulseError::message(format!(
        "Trace service did not become healthy within {}s.\n\
         Check server logs or start it manually.",
        HEALTH_TIMEOUT.as_secs(),
    )))
}

fn apply_server_env(config: &PulseConfig, command: &mut Command, base_url: &Url) {
    let auth_secret = config
        .server_auth_secret
        .clone()
        .unwrap_or_else(random_secret);
    let encryption_key = config
        .server_encryption_key
        .clone()
        .unwrap_or_else(random_secret);

    command.env("BETTER_AUTH_SECRET", &auth_secret);
    command.env("ENCRYPTION_KEY", &encryption_key);

    if std::env::var_os("BETTER_AUTH_URL").is_none() {
        command.env("BETTER_AUTH_URL", base_url.origin().ascii_serialization());
    }
    if std::env::var_os("PORT").is_none() {
        if let Some(port) = base_url.port_or_known_default() {
            command.env("PORT", port.to_string());
        }
    }
}

pub fn random_secret() -> String {
    format!(
        "{}{}",
        Uuid::new_v4().as_simple(),
        Uuid::new_v4().as_simple()
    )
}

pub async fn is_healthy(client: &Client, base_url: &Url) -> bool {
    match make_url(base_url, "/health") {
        Ok(url) => match client.get(url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

async fn wait_until_healthy(
    client: &Client,
    base_url: &Url,
    timeout: Duration,
    interval: Duration,
) -> bool {
    let mut elapsed = Duration::from_secs(0);
    while elapsed <= timeout {
        if is_healthy(client, base_url).await {
            return true;
        }
        sleep(interval).await;
        elapsed = elapsed.saturating_add(interval);
    }
    false
}

fn make_url(base_url: &Url, path: &str) -> Result<Url> {
    base_url
        .join(path.trim_start_matches('/'))
        .map_err(|err| PulseError::message(format!("invalid url path: {err}")))
}

fn normalize_base_url(raw: &str) -> Result<Url> {
    let trimmed = raw.trim().trim_end_matches('/');
    Url::parse(trimmed).map_err(|err| PulseError::message(format!("invalid API url: {err}")))
}

fn is_local_host(url: &Url) -> bool {
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
}
