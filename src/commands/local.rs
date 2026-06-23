use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use clap::Args;
use reqwest::{Client, Url};
use tokio::time::sleep;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use crate::{
    config::{ConfigMode, ConfigStore, PulseConfig},
    error::{PulseError, Result},
};

use super::setup::ensure_local_config;

const DEFAULT_API_URL: &str = "http://localhost:3000";
const DEFAULT_SERVER_COMMAND: &str = "pulse-server";
const HEALTH_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_INTERVAL: Duration = Duration::from_millis(500);
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const FOLLOW_INTERVAL: Duration = Duration::from_millis(500);
const DEFAULT_LOG_LINES: usize = 100;
const USER_AGENT: &str = concat!("pulse-cli/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Args)]
pub struct UpArgs {
    /// Open the dashboard after the local server becomes healthy
    #[arg(long)]
    pub open: bool,
}

#[derive(Debug, Args)]
pub struct LogsArgs {
    /// Number of recent lines to print before exiting/following
    #[arg(long, default_value_t = DEFAULT_LOG_LINES)]
    pub lines: usize,
    /// Stream new log lines as they are written
    #[arg(long)]
    pub follow: bool,
}

pub async fn run_up(args: UpArgs) -> Result<()> {
    let (config, needs_setup) = match load_local_config() {
        Ok(config) => (config, false),
        Err(PulseError::ConfigMissing) => {
            println!("Pulse is not initialized. Running first-time local setup...");
            (default_local_config(), true)
        }
        Err(err) => return Err(err),
    };
    let base_url = normalize_base_url(&config.api_url)?;
    let client = http_client()?;

    if !is_local_host(&base_url) {
        return Err(PulseError::message(format!(
            "pulse up requires a loopback local API URL. Got: {base_url}"
        )));
    }

    let pid_path = ConfigStore::server_pid_path()?;
    let log_path = ConfigStore::server_log_path()?;
    let server_command = config
        .server_command
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(DEFAULT_SERVER_COMMAND)
        .trim()
        .to_string();

    if let Some(pid) = read_pid(&pid_path)? {
        if process_exists(pid) {
            if is_healthy(&client, &base_url).await {
                let config =
                    ensure_config_after_start(needs_setup, &config, &server_command).await?;
                print_running_status(&config, pid, &log_path);
                maybe_open_dashboard(args.open, &config.api_url)?;
                return Ok(());
            }
            return Err(PulseError::message(format!(
                "Pulse server process {pid} is running but unhealthy. Check `pulse logs` or run `pulse restart`."
            )));
        } else {
            remove_stale_pid(&pid_path)?;
        }
    }

    ensure_parent_dir(&pid_path)?;
    ensure_parent_dir(&log_path)?;

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let log_file_err = log_file.try_clone()?;

    let mut command = Command::new(&server_command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err));

    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    apply_server_env_defaults(&mut command, &base_url);

    let child = command
        .spawn()
        .map_err(|err| PulseError::message(format!("failed to start `{server_command}`: {err}")))?;
    let pid = child.id();
    write_pid(&pid_path, pid)?;

    if wait_until_healthy(&client, &base_url, HEALTH_TIMEOUT, HEALTH_INTERVAL).await {
        let config = ensure_config_after_start(needs_setup, &config, &server_command).await?;
        print_running_status(&config, pid, &log_path);
        maybe_open_dashboard(args.open, &config.api_url)?;
        return Ok(());
    }

    remove_stale_pid(&pid_path)?;
    Err(PulseError::message(format!(
        "Pulse server did not become healthy within {}s. Check `pulse logs`.",
        HEALTH_TIMEOUT.as_secs()
    )))
}

fn default_local_config() -> PulseConfig {
    PulseConfig {
        mode: Some(ConfigMode::Local),
        api_url: DEFAULT_API_URL.to_string(),
        api_key: String::new(),
        project_id: String::new(),
        server_command: Some(DEFAULT_SERVER_COMMAND.to_string()),
        local_email: None,
        local_password: None,
    }
}

async fn ensure_config_after_start(
    needs_setup: bool,
    config: &PulseConfig,
    server_command: &str,
) -> Result<PulseConfig> {
    if !needs_setup {
        return Ok(config.clone());
    }

    let config = ensure_local_config(&config.api_url, server_command, false).await?;
    println!("Local Pulse initialized. Run `pulse install-hooks` to capture agent events.");
    Ok(config)
}

pub async fn run_down() -> Result<()> {
    let config = load_local_config()?;
    let pid_path = ConfigStore::server_pid_path()?;

    let Some(pid) = read_pid(&pid_path)? else {
        println!("Pulse server is not running.");
        return Ok(());
    };

    if !process_exists(pid) {
        remove_stale_pid(&pid_path)?;
        println!("Removed stale Pulse server PID file.");
        return Ok(());
    }

    terminate_process(pid)?;

    let mut elapsed = Duration::from_secs(0);
    while elapsed < HEALTH_TIMEOUT {
        if !process_exists(pid) {
            remove_stale_pid(&pid_path)?;
            println!("Stopped Pulse server at {}.", config.api_url);
            return Ok(());
        }
        sleep(HEALTH_INTERVAL).await;
        elapsed = elapsed.saturating_add(HEALTH_INTERVAL);
    }

    force_terminate_process(pid)?;
    remove_stale_pid(&pid_path)?;
    println!("Force-stopped Pulse server at {}.", config.api_url);
    Ok(())
}

pub async fn run_restart(args: UpArgs) -> Result<()> {
    let _ = run_down().await;
    run_up(args).await
}

pub async fn run_logs(args: LogsArgs) -> Result<()> {
    let _config = load_local_config()?;
    let log_path = ConfigStore::server_log_path()?;

    if !log_path.exists() {
        return Err(PulseError::message(format!(
            "Log file not found at {}. Start the local server with `pulse up` first.",
            log_path.display()
        )));
    }

    print_tail(&log_path, args.lines)?;

    if args.follow {
        follow_file(&log_path).await?;
    }

    Ok(())
}

pub struct LocalServerStatus {
    pub mode: ConfigMode,
    pub pid: Option<u32>,
    pub running: bool,
    pub healthy: bool,
    pub pid_path: std::path::PathBuf,
    pub log_path: std::path::PathBuf,
}

pub async fn local_server_status(config: &PulseConfig) -> Result<LocalServerStatus> {
    let pid_path = ConfigStore::server_pid_path()?;
    let log_path = ConfigStore::server_log_path()?;
    let base_url = normalize_base_url(&config.api_url)?;
    let client = http_client()?;

    let pid = read_pid(&pid_path)?;
    let running = pid.is_some_and(process_exists);
    let healthy = if running {
        is_healthy(&client, &base_url).await
    } else {
        false
    };

    Ok(LocalServerStatus {
        mode: config.effective_mode(),
        pid,
        running,
        healthy,
        pid_path,
        log_path,
    })
}

fn load_local_config() -> Result<PulseConfig> {
    let config = ConfigStore::load()?;
    if config.effective_mode() != ConfigMode::Local {
        return Err(PulseError::message(
            "Current configuration points to a remote Pulse instance. Use `pulse connect` to change targets or `pulse setup --local` for a managed local server.",
        ));
    }
    Ok(config)
}

fn http_client() -> Result<Client> {
    Client::builder()
        .user_agent(USER_AGENT)
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(Into::into)
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        PulseError::message(format!(
            "unable to resolve parent directory for {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent)?;
    Ok(())
}

fn write_pid(path: &Path, pid: u32) -> Result<()> {
    fs::write(path, format!("{pid}\n"))?;
    Ok(())
}

fn read_pid(path: &Path) -> Result<Option<u32>> {
    match fs::read_to_string(path) {
        Ok(contents) => match contents.trim().parse::<u32>() {
            Ok(pid) => Ok(Some(pid)),
            Err(_) => Ok(None),
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn remove_stale_pid(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn process_exists(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}")])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .map(|output| {
                let body = String::from_utf8_lossy(&output.stdout);
                body.contains(&pid.to_string())
            })
            .unwrap_or(false)
    }
}

fn terminate_process(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .arg(pid.to_string())
            .status()
            .map_err(|err| PulseError::message(format!("failed to stop process {pid}: {err}")))?;
        if !status.success() {
            return Err(PulseError::message(format!(
                "failed to stop process {pid}: kill exited with {status}"
            )));
        }
    }

    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .status()
            .map_err(|err| PulseError::message(format!("failed to stop process {pid}: {err}")))?;
        if !status.success() {
            return Err(PulseError::message(format!(
                "failed to stop process {pid}: taskkill exited with {status}"
            )));
        }
    }

    Ok(())
}

fn force_terminate_process(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status()
            .map_err(|err| {
                PulseError::message(format!("failed to force-stop process {pid}: {err}"))
            })?;
        if !status.success() {
            return Err(PulseError::message(format!(
                "failed to force-stop process {pid}: kill -9 exited with {status}"
            )));
        }
    }

    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F", "/T"])
            .status()
            .map_err(|err| {
                PulseError::message(format!("failed to force-stop process {pid}: {err}"))
            })?;
        if !status.success() {
            return Err(PulseError::message(format!(
                "failed to force-stop process {pid}: taskkill exited with {status}"
            )));
        }
    }

    Ok(())
}

fn apply_server_env_defaults(command: &mut Command, base_url: &Url) {
    if std::env::var_os("BETTER_AUTH_SECRET").is_none() {
        command.env("BETTER_AUTH_SECRET", random_secret());
    }
    if std::env::var_os("ENCRYPTION_KEY").is_none() {
        command.env("ENCRYPTION_KEY", random_secret());
    }
    if std::env::var_os("BETTER_AUTH_URL").is_none() {
        command.env("BETTER_AUTH_URL", base_url.origin().ascii_serialization());
    }
    if std::env::var_os("PORT").is_none() {
        if let Some(port) = base_url.port_or_known_default() {
            command.env("PORT", port.to_string());
        }
    }
}

fn random_secret() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().as_simple(),
        uuid::Uuid::new_v4().as_simple()
    )
}

fn normalize_base_url(raw: &str) -> Result<Url> {
    let trimmed = raw.trim().trim_end_matches('/');
    Url::parse(trimmed).map_err(|err| PulseError::message(format!("invalid API url: {err}")))
}

fn is_local_host(url: &Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "::1" | "[::1]")
    )
}

async fn is_healthy(client: &Client, base_url: &Url) -> bool {
    let url = match base_url.join("health") {
        Ok(url) => url,
        Err(_) => return false,
    };

    match client.get(url).send().await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

async fn wait_until_healthy(
    client: &Client,
    base_url: &Url,
    timeout: Duration,
    interval: Duration,
) -> bool {
    let mut elapsed = Duration::ZERO;
    while elapsed <= timeout {
        if is_healthy(client, base_url).await {
            return true;
        }
        sleep(interval).await;
        elapsed = elapsed.saturating_add(interval);
    }
    false
}

fn print_running_status(config: &PulseConfig, pid: u32, log_path: &Path) {
    println!("Pulse server is running.");
    println!("  API URL       : {}", config.api_url);
    println!("  Dashboard URL : {}", config.api_url);
    println!("  PID           : {pid}");
    println!("  Logs          : {}", log_path.display());
}

fn maybe_open_dashboard(open: bool, url: &str) -> Result<()> {
    if !open {
        return Ok(());
    }

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

fn print_tail(path: &Path, lines: usize) -> Result<()> {
    let contents = fs::read_to_string(path)?;
    let line_buffer = contents.lines().collect::<Vec<_>>();
    let start = line_buffer.len().saturating_sub(lines);
    for line in &line_buffer[start..] {
        println!("{line}");
    }
    Ok(())
}

async fn follow_file(path: &Path) -> Result<()> {
    let mut file = File::open(path)?;
    let mut offset = file.metadata()?.len();

    loop {
        let current_len = file.metadata()?.len();
        if current_len < offset {
            offset = 0;
        }

        if current_len > offset {
            file.seek(SeekFrom::Start(offset))?;
            let mut buffer = String::new();
            file.read_to_string(&mut buffer)?;
            print!("{buffer}");
            io::stdout().flush()?;
            offset = current_len;
        }

        sleep(FOLLOW_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{read_pid, remove_stale_pid, write_pid};

    #[test]
    fn pid_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("server.pid");

        write_pid(&path, 4242).unwrap();
        assert_eq!(read_pid(&path).unwrap(), Some(4242));
    }

    #[test]
    fn remove_stale_pid_is_idempotent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("server.pid");

        fs::write(&path, "100\n").unwrap();
        remove_stale_pid(&path).unwrap();
        remove_stale_pid(&path).unwrap();

        assert!(!path.exists());
    }
}
