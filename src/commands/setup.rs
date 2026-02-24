use std::{
    io::{self, Write},
    process::{Command, Stdio},
    time::Duration,
};

use clap::Args;
use reqwest::{
    Client, Url,
    header::{COOKIE, HeaderMap, HeaderValue, SET_COOKIE},
};
use serde::Deserialize;
use serde_json::json;
use tokio::time::sleep;
use uuid::Uuid;

use crate::{
    config::{ConfigStore, PulseConfig},
    error::{PulseError, Result},
};

use super::run_connect;

const DEFAULT_API_URL: &str = "http://localhost:3000";
const DEFAULT_SERVER_COMMAND: &str = "pulse-server";
const DEFAULT_PROJECT_NAME: &str = "Pulse Project";
const DEFAULT_LOCAL_ACCOUNT_NAME: &str = "Local User";
const HEALTH_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_INTERVAL: Duration = Duration::from_millis(500);
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const USER_AGENT: &str = concat!("pulse-cli/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Args)]
pub struct SetupArgs {
    /// Trace service URL (defaults to http://localhost:3000)
    #[arg(long)]
    pub api_url: Option<String>,
    /// User display name
    #[arg(long)]
    pub name: Option<String>,
    /// Account email
    #[arg(long)]
    pub email: Option<String>,
    /// Account password
    #[arg(long)]
    pub password: Option<String>,
    /// Configure local mode with generated/reused local credentials
    #[arg(long)]
    pub local: bool,
    /// Print the full API key in setup output
    #[arg(long)]
    pub show_api_key: bool,
    /// Project name (creates if missing)
    #[arg(long)]
    pub project_name: Option<String>,
    /// Server command to start when local service is not reachable
    #[arg(long, default_value = DEFAULT_SERVER_COMMAND)]
    pub server_command: String,
    /// Do not attempt to start pulse-server automatically
    #[arg(long)]
    pub no_start_server: bool,
    /// Skip automatic `pulse connect` at the end
    #[arg(long)]
    pub no_connect: bool,
}

#[derive(Debug, Deserialize)]
struct ProjectsResponse {
    projects: Vec<ProjectSummary>,
}

#[derive(Debug, Deserialize)]
struct ProjectSummary {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct KeysResponse {
    keys: Vec<ApiKeySummary>,
}

#[derive(Debug, Deserialize)]
struct ApiKeySummary {
    key: String,
}

#[derive(Debug, Deserialize)]
struct CreateProjectResponse {
    #[serde(rename = "projectId")]
    project_id: String,
    #[serde(rename = "apiKey")]
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct CreateApiKeyResponse {
    #[serde(rename = "apiKey")]
    api_key: String,
}

pub async fn run_setup(args: SetupArgs) -> Result<()> {
    println!("Pulse setup");
    println!("-----------");

    let SetupArgs {
        api_url,
        name,
        email,
        password,
        local,
        show_api_key,
        project_name,
        server_command,
        no_start_server,
        no_connect,
    } = args;

    let existing_config = ConfigStore::load().ok();

    let api_url = match (api_url, local) {
        (Some(value), _) => value,
        (None, true) => DEFAULT_API_URL.to_string(),
        (None, false) => prompt_with_default("Trace service URL", DEFAULT_API_URL)?,
    };
    let base_url = normalize_base_url(&api_url)?;
    if local && !is_local_host(&base_url) {
        return Err(PulseError::message(format!(
            "--local requires a loopback API URL. Got: {base_url}",
        )));
    }

    let name = match (name, local) {
        (Some(value), _) => value,
        (None, true) => DEFAULT_LOCAL_ACCOUNT_NAME.to_string(),
        (None, false) => prompt_required("Account name", false)?,
    };
    let project_name = match (project_name, local) {
        (Some(value), _) => value,
        (None, true) => DEFAULT_PROJECT_NAME.to_string(),
        (None, false) => prompt_with_default("Project name", DEFAULT_PROJECT_NAME)?,
    };

    let (email, password) = if local {
        let persisted_pair = existing_config.as_ref().and_then(|cfg| {
            let email = cfg.local_email.clone()?;
            let password = cfg.local_password.clone()?;
            Some((email, password))
        });

        let local_email = email
            .or_else(|| persisted_pair.as_ref().map(|(value, _)| value.clone()))
            .unwrap_or_else(generate_local_email);
        let local_password = password
            .or_else(|| persisted_pair.as_ref().map(|(_, value)| value.clone()))
            .unwrap_or_else(random_secret);
        println!("Using local setup mode with managed local credentials.");
        (local_email, local_password)
    } else {
        let account_email = match email {
            Some(value) => value,
            None => prompt_required("Account email", false)?,
        };
        let account_password = match password {
            Some(value) => value,
            None => prompt_required("Account password", true)?,
        };
        (account_email, account_password)
    };

    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(HTTP_TIMEOUT)
        .build()?;

    ensure_trace_service(&client, &base_url, &server_command, no_start_server).await?;

    let session_cookie =
        ensure_session_cookie(&client, &base_url, &name, &email, &password, &project_name).await?;

    let (project_id, api_key) =
        resolve_project_and_api_key(&client, &base_url, &session_cookie, &project_name).await?;

    let config = PulseConfig {
        api_url: base_url.to_string(),
        api_key,
        project_id,
        local_email: local.then(|| email.clone()),
        local_password: local.then(|| password.clone()),
    }
    .sanitized();

    ConfigStore::save(&config)?;
    let config_path = ConfigStore::config_path()?;
    println!("Saved configuration to {}", config_path.display());
    println!("API URL: {}", config.api_url);
    println!("Project ID: {}", config.project_id);
    println!(
        "API Key: {}",
        format_api_key_for_display(&config.api_key, show_api_key)
    );
    if local && !show_api_key {
        println!("Use `pulse setup --local --show-api-key` to print the full API key.");
    }

    if no_connect {
        println!("Skipped agent integration setup (--no-connect).");
    } else {
        println!("Installing agent integrations...");
        run_connect()?;
    }

    println!("Setup complete.");
    println!("Run `pulse status` to verify connectivity and hooks.");

    Ok(())
}

async fn ensure_trace_service(
    client: &Client,
    base_url: &Url,
    server_command: &str,
    no_start_server: bool,
) -> Result<()> {
    if is_healthy(client, base_url).await {
        println!("Trace service reachable at {}", base_url);
        return Ok(());
    }

    if no_start_server {
        return Err(PulseError::message(format!(
            "Trace service is not reachable at {}. Start it manually with `{}` and retry.",
            base_url, server_command
        )));
    }

    if !is_local_host(base_url) {
        return Err(PulseError::message(format!(
            "Trace service is not reachable at {} and this is not a local URL. \
             Start your remote service manually or use --api-url pointing to a reachable instance.",
            base_url
        )));
    }

    println!(
        "Trace service is not reachable. Starting `{}` in the background...",
        server_command
    );

    let mut command = Command::new(server_command.trim());
    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    let used_defaults = apply_server_env_defaults(&mut command, base_url);
    let child = command.spawn().map_err(|err| {
        PulseError::message(format!(
            "Failed to start `{}`: {err}",
            server_command.trim()
        ))
    })?;

    println!("Started `{}` (pid={}).", server_command.trim(), child.id());
    if used_defaults {
        println!("Using generated local auth/encryption secrets for this server process.");
    }

    if wait_until_healthy(client, base_url, HEALTH_TIMEOUT, HEALTH_INTERVAL).await {
        println!("Trace service is ready at {}", base_url);
        return Ok(());
    }

    Err(PulseError::message(format!(
        "Trace service did not become healthy within {}s. \
         Check server logs or start `{}` manually.",
        HEALTH_TIMEOUT.as_secs(),
        server_command.trim()
    )))
}

fn apply_server_env_defaults(command: &mut Command, base_url: &Url) -> bool {
    let mut used_defaults = false;

    if std::env::var_os("BETTER_AUTH_SECRET").is_none() {
        command.env("BETTER_AUTH_SECRET", random_secret());
        used_defaults = true;
    }
    if std::env::var_os("ENCRYPTION_KEY").is_none() {
        command.env("ENCRYPTION_KEY", random_secret());
        used_defaults = true;
    }
    if std::env::var_os("BETTER_AUTH_URL").is_none() {
        command.env("BETTER_AUTH_URL", base_url.origin().ascii_serialization());
    }
    if std::env::var_os("PORT").is_none()
        && let Some(port) = base_url.port_or_known_default()
    {
        command.env("PORT", port.to_string());
    }

    used_defaults
}

fn random_secret() -> String {
    format!(
        "{}{}",
        Uuid::new_v4().as_simple(),
        Uuid::new_v4().as_simple()
    )
}

fn generate_local_email() -> String {
    let random = Uuid::new_v4().simple().to_string();
    format!("local-{}@pulse.local", &random[..12])
}

fn format_api_key_for_display(api_key: &str, show_full: bool) -> String {
    if show_full {
        return api_key.to_string();
    }

    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return "(empty)".to_string();
    }
    if trimmed.len() <= 12 {
        return "(hidden)".to_string();
    }

    format!(
        "{}...{}",
        &trimmed[..8],
        &trimmed[trimmed.len().saturating_sub(4)..]
    )
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

async fn is_healthy(client: &Client, base_url: &Url) -> bool {
    match make_url(base_url, "/health") {
        Ok(url) => match client.get(url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

async fn ensure_session_cookie(
    client: &Client,
    base_url: &Url,
    name: &str,
    email: &str,
    password: &str,
    project_name: &str,
) -> Result<String> {
    if let Some(cookie) = sign_in(client, base_url, email, password).await? {
        println!("Signed in existing account.");
        return Ok(cookie);
    }

    println!("Creating account and first project...");
    sign_up_with_project(client, base_url, name, email, password, project_name).await?;

    match sign_in(client, base_url, email, password).await? {
        Some(cookie) => {
            println!("Signed in.");
            Ok(cookie)
        }
        None => Err(PulseError::message(
            "Account was created but sign-in failed. Re-run `pulse setup` with --email/--password.",
        )),
    }
}

async fn sign_in(
    client: &Client,
    base_url: &Url,
    email: &str,
    password: &str,
) -> Result<Option<String>> {
    let url = make_url(base_url, "/api/auth/sign-in/email")?;
    let response = client
        .post(url)
        .json(&json!({
            "email": email.trim(),
            "password": password,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let cookie = extract_session_cookie(response.headers()).ok_or_else(|| {
        PulseError::message("Sign-in succeeded but no session cookie was returned by the server")
    })?;

    Ok(Some(cookie))
}

async fn sign_up_with_project(
    client: &Client,
    base_url: &Url,
    name: &str,
    email: &str,
    password: &str,
    project_name: &str,
) -> Result<()> {
    let url = make_url(base_url, "/dashboard/api/signup")?;
    let response = client
        .post(url)
        .json(&json!({
            "name": name.trim(),
            "email": email.trim().to_lowercase(),
            "password": password,
            "projectName": project_name.trim(),
        }))
        .send()
        .await?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(PulseError::message(format!(
        "Sign-up failed ({status}): {}",
        compact_body(&body)
    )))
}

async fn resolve_project_and_api_key(
    client: &Client,
    base_url: &Url,
    session_cookie: &str,
    project_name: &str,
) -> Result<(String, String)> {
    let projects = get_projects(client, base_url, session_cookie).await?;
    if let Some(project) = projects
        .iter()
        .find(|project| project.name.trim() == project_name.trim())
    {
        println!("Using existing project `{}`.", project.name);
        let api_key = get_or_create_api_key(client, base_url, session_cookie, &project.id).await?;
        return Ok((project.id.clone(), api_key));
    }

    println!("Creating project `{}`...", project_name.trim());
    let created = create_project(client, base_url, session_cookie, project_name).await?;
    Ok((created.project_id, created.api_key))
}

async fn get_projects(
    client: &Client,
    base_url: &Url,
    session_cookie: &str,
) -> Result<Vec<ProjectSummary>> {
    let url = make_url(base_url, "/dashboard/api/projects")?;
    let response = client
        .get(url)
        .header(COOKIE, cookie_header_value(session_cookie)?)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(PulseError::message(format!(
            "Failed to list projects ({status}): {}",
            compact_body(&body)
        )));
    }

    let payload: ProjectsResponse = response.json().await?;
    Ok(payload.projects)
}

async fn create_project(
    client: &Client,
    base_url: &Url,
    session_cookie: &str,
    project_name: &str,
) -> Result<CreateProjectResponse> {
    let url = make_url(base_url, "/dashboard/api/projects")?;
    let response = client
        .post(url)
        .header(COOKIE, cookie_header_value(session_cookie)?)
        .json(&json!({ "name": project_name.trim() }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(PulseError::message(format!(
            "Failed to create project ({status}): {}",
            compact_body(&body)
        )));
    }

    response.json().await.map_err(Into::into)
}

async fn get_or_create_api_key(
    client: &Client,
    base_url: &Url,
    session_cookie: &str,
    project_id: &str,
) -> Result<String> {
    if let Some(existing) = list_api_keys(client, base_url, session_cookie, project_id)
        .await?
        .into_iter()
        .next()
    {
        return Ok(existing.key);
    }

    create_api_key(client, base_url, session_cookie, project_id).await
}

async fn list_api_keys(
    client: &Client,
    base_url: &Url,
    session_cookie: &str,
    project_id: &str,
) -> Result<Vec<ApiKeySummary>> {
    let url = make_url(base_url, "/dashboard/api/api-keys")?;
    let response = client
        .get(url)
        .header(COOKIE, cookie_header_value(session_cookie)?)
        .header("X-Project-Id", project_id.trim())
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(PulseError::message(format!(
            "Failed to list API keys ({status}): {}",
            compact_body(&body)
        )));
    }

    let payload: KeysResponse = response.json().await?;
    Ok(payload.keys)
}

async fn create_api_key(
    client: &Client,
    base_url: &Url,
    session_cookie: &str,
    project_id: &str,
) -> Result<String> {
    let url = make_url(base_url, "/dashboard/api/api-keys")?;
    let response = client
        .post(url)
        .header(COOKIE, cookie_header_value(session_cookie)?)
        .header("X-Project-Id", project_id.trim())
        .json(&json!({ "name": "CLI Key" }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(PulseError::message(format!(
            "Failed to create API key ({status}): {}",
            compact_body(&body)
        )));
    }

    let payload: CreateApiKeyResponse = response.json().await?;
    Ok(payload.api_key)
}

fn cookie_header_value(session_cookie: &str) -> Result<HeaderValue> {
    HeaderValue::from_str(session_cookie.trim())
        .map_err(|err| PulseError::message(format!("invalid session cookie: {err}")))
}

fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(extract_cookie_pair)
}

fn extract_cookie_pair(set_cookie: &str) -> Option<String> {
    let prefix = "better-auth.session_token=";
    let start = set_cookie.find(prefix)?;
    let suffix = &set_cookie[start..];
    let pair = suffix.split(';').next()?.trim();
    if pair.starts_with(prefix) && !pair.is_empty() {
        Some(pair.to_string())
    } else {
        None
    }
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

fn prompt_required(prompt: &str, secret: bool) -> Result<String> {
    loop {
        let value = if secret {
            rpassword::prompt_password(format!("{prompt}: "))?
        } else {
            print!("{prompt}: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        };

        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
        println!("Value required");
    }
}

fn prompt_with_default(prompt: &str, default: &str) -> Result<String> {
    print!("{prompt} [{default}]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let value = input.trim();
    if value.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(value.to_string())
    }
}
