use std::process::Command;
use std::time::Duration;

use clap::Args;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};

use crate::config::ConfigStore;
use crate::error::{PulseError, Result};

const DEFAULT_DASHBOARD_URL: &str = "http://localhost:5173";
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const USER_AGENT: &str = concat!("pulse-cli/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Args)]
pub struct DashboardArgs {
    /// Trace service URL (defaults to configured value)
    #[arg(long)]
    pub api_url: Option<String>,
    /// Dashboard frontend URL to open after local login handoff
    #[arg(long)]
    pub dashboard_url: Option<String>,
    /// Print the login URL instead of opening a browser
    #[arg(long)]
    pub no_open: bool,
}

#[derive(Debug, Serialize)]
struct LocalLoginTokenRequest<'a> {
    email: &'a str,
    password: &'a str,
    redirect_url: &'a str,
}

#[derive(Debug, Deserialize)]
struct LocalLoginTokenResponse {
    login_url: String,
    expires_at: String,
}

pub async fn run_dashboard(args: DashboardArgs) -> Result<()> {
    let config = ConfigStore::load()?;
    let api_url = args.api_url.unwrap_or_else(|| config.api_url.clone());
    let dashboard_url = args
        .dashboard_url
        .unwrap_or_else(|| DEFAULT_DASHBOARD_URL.to_string());

    let base_url = normalize_base_url(&api_url)?;
    let dashboard_url = normalize_base_url(&dashboard_url)?;

    if !is_local_host(&base_url) {
        return Err(PulseError::message(format!(
            "pulse dashboard requires a local API URL. Got: {base_url}"
        )));
    }
    if !is_local_host(&dashboard_url) {
        return Err(PulseError::message(format!(
            "pulse dashboard requires a local dashboard URL. Got: {dashboard_url}"
        )));
    }

    let local_email = config.local_email.ok_or_else(|| {
        PulseError::message(
            "Local dashboard auto-login is not configured. Run `pulse setup --local` first.",
        )
    })?;
    let local_password = config.local_password.ok_or_else(|| {
        PulseError::message(
            "Local dashboard auto-login is not configured. Run `pulse setup --local` first.",
        )
    })?;

    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(HTTP_TIMEOUT)
        .build()?;

    let health_url = make_url(&base_url, "/health")?;
    client.get(health_url).send().await?.error_for_status()?;

    let token_url = make_url(&base_url, "/dashboard/api/local-login-token")?;
    let payload = LocalLoginTokenRequest {
        email: local_email.trim(),
        password: local_password.trim(),
        redirect_url: dashboard_url.as_str(),
    };

    let response = client.post(token_url).json(&payload).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(PulseError::message(format!(
            "Failed to create local login token ({status}): {}",
            compact_body(&body)
        )));
    }

    let token_response: LocalLoginTokenResponse = response.json().await?;
    println!(
        "Local dashboard login token created (expires: {}).",
        token_response.expires_at
    );

    if args.no_open {
        println!("Open this URL in your browser:");
        println!("{}", token_response.login_url);
        return Ok(());
    }

    match open_in_browser(&token_response.login_url) {
        Ok(()) => {
            println!("Opened dashboard in your browser.");
            println!("If it did not open, use:");
            println!("{}", token_response.login_url);
            Ok(())
        }
        Err(err) => {
            println!("Could not open a browser automatically: {err}");
            println!("Open this URL manually:");
            println!("{}", token_response.login_url);
            Ok(())
        }
    }
}

fn open_in_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut cmd = Command::new("open");
        cmd.arg(url);
        cmd
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", url]);
        cmd
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(url);
        cmd
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|err| PulseError::message(format!("failed to launch browser: {err}")))
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

fn compact_body(body: &str) -> String {
    let collapsed = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() <= 240 {
        collapsed
    } else {
        format!("{}...", &collapsed[..240])
    }
}
