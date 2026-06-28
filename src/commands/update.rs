use std::{
    env, fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use clap::Args;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{
    config::{ConfigMode, ConfigStore},
    error::{PulseError, Result},
};

const CLI_REPO: &str = "EK-LABS-LLC/trace-cli";
const SERVER_REPO: &str = "EK-LABS-LLC/trace-service";
const CLI_INSTALL_URL: &str =
    "https://raw.githubusercontent.com/EK-LABS-LLC/trace-cli/main/install.sh";
const SERVER_INSTALL_URL: &str =
    "https://raw.githubusercontent.com/EK-LABS-LLC/trace-service/main/scripts/install.sh";
const SERVER_BINARY: &str = "pulse-server";
const INSTALL_METADATA_FILE: &str = ".pulse-install.toml";
const UPDATE_STATE_FILE: &str = "update-state.toml";

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Install without asking for confirmation
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

#[derive(Debug)]
struct UpdateStatus {
    latest_cli: String,
    current_cli: &'static str,
    latest_server: String,
    current_server: Option<String>,
    local_managed: bool,
}

impl UpdateStatus {
    fn cli_update_available(&self) -> bool {
        is_newer_version(&self.latest_cli, self.current_cli)
    }

    fn server_update_available(&self) -> bool {
        self.local_managed
            && self
                .current_server
                .as_deref()
                .map(|current| is_newer_version(&self.latest_server, current))
                .unwrap_or(true)
    }

    fn update_available(&self) -> bool {
        self.cli_update_available() || self.server_update_available()
    }

    fn target(&self) -> UpdateTarget {
        UpdateTarget {
            latest_cli: self.latest_cli.clone(),
            latest_server: if self.local_managed {
                Some(self.latest_server.clone())
            } else {
                None
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct UpdateTarget {
    latest_cli: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    latest_server: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UpdateState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dismissed: Option<UpdateTarget>,
}

pub async fn run_update(args: UpdateArgs) -> Result<()> {
    let status = update_status().await?;

    if !status.update_available() {
        println!("Pulse is already up to date.");
        println!("CLI: {}", status.current_cli);
        if status.local_managed {
            println!(
                "Server: {}",
                status.current_server.as_deref().unwrap_or("unknown")
            );
        }
        return Ok(());
    }

    print_update_summary(&status);
    if !args.yes && !confirm("Update now?")? {
        println!("Skipped update.");
        return Ok(());
    }

    install_updates(&status)
}

pub async fn maybe_prompt_update() {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return;
    }

    if env::var_os("PULSE_SKIP_UPDATE_CHECK").is_some() {
        return;
    }

    let Ok(status) = update_status().await else {
        return;
    };

    if !status.update_available() {
        return;
    }

    if update_dismissed(&status) {
        return;
    }

    print_update_summary(&status);
    match confirm("Update now?") {
        Ok(true) => {
            if let Err(err) = install_updates(&status) {
                let _ = writeln!(io::stderr(), "Pulse update failed: {err}");
            }
        }
        Ok(false) => {
            if let Err(err) = dismiss_update(&status) {
                let _ = writeln!(io::stderr(), "Could not save update preference: {err}");
            }
            let _ = writeln!(io::stderr(), "Skipping update.");
        }
        Err(_) => {}
    }
}

async fn update_status() -> Result<UpdateStatus> {
    let latest_cli = latest_release(CLI_REPO).await?;
    let latest_server = latest_release(SERVER_REPO).await?;
    let local_managed = is_local_managed();
    let current_server = if local_managed {
        installed_server_version()
    } else {
        None
    };

    Ok(UpdateStatus {
        latest_cli,
        current_cli: current_version(),
        latest_server,
        current_server,
        local_managed,
    })
}

async fn latest_release(repo: &str) -> Result<String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let release = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?
        .get(url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "pulse-cli")
        .send()
        .await?
        .error_for_status()?
        .json::<GitHubRelease>()
        .await?;

    Ok(release.tag_name)
}

fn print_update_summary(status: &UpdateStatus) {
    eprintln!("Pulse updates are available:");
    if status.cli_update_available() {
        eprintln!("  CLI: {} -> {}", status.current_cli, status.latest_cli);
    }
    if status.server_update_available() {
        eprintln!(
            "  Server/dashboard: {} -> {}",
            status.current_server.as_deref().unwrap_or("unknown"),
            status.latest_server
        );
    }
}

fn install_updates(status: &UpdateStatus) -> Result<()> {
    if status.local_managed {
        println!("Updating Pulse server, dashboard assets, and CLI...");
        run_shell_install(&format!(
            "curl -fsSL {SERVER_INSTALL_URL} | bash -s -- pulse-server"
        ))?;
        println!("Update complete. If Pulse server is already running, run `pulse restart`.");
        return Ok(());
    }

    println!("Updating Pulse CLI...");
    run_shell_install(&format!("curl -fsSL {CLI_INSTALL_URL} | sh"))
}

fn run_shell_install(command: &str) -> Result<()> {
    let status = Command::new("sh").arg("-c").arg(command).status()?;

    if !status.success() {
        return Err(PulseError::message(format!(
            "installer exited with status {status}"
        )));
    }

    Ok(())
}

fn confirm(prompt: &str) -> Result<bool> {
    eprint!("{prompt} [Y/n] ");
    io::stderr().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim().to_ascii_lowercase();

    Ok(answer.is_empty() || answer == "y" || answer == "yes")
}

fn update_state_path() -> Result<PathBuf> {
    Ok(ConfigStore::config_dir()?.join(UPDATE_STATE_FILE))
}

fn load_update_state() -> UpdateState {
    let Ok(path) = update_state_path() else {
        return UpdateState::default();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return UpdateState::default();
    };
    toml::from_str(&contents).unwrap_or_default()
}

fn save_update_state(state: &UpdateState) -> Result<()> {
    let path = update_state_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, toml::to_string_pretty(state)?)?;
    Ok(())
}

fn update_dismissed(status: &UpdateStatus) -> bool {
    dismissed_matches(&load_update_state(), status)
}

fn dismissed_matches(state: &UpdateState, status: &UpdateStatus) -> bool {
    state
        .dismissed
        .as_ref()
        .map(|dismissed| dismissed == &status.target())
        .unwrap_or(false)
}

fn dismiss_update(status: &UpdateStatus) -> Result<()> {
    save_update_state(&UpdateState {
        dismissed: Some(status.target()),
    })
}

fn is_local_managed() -> bool {
    ConfigStore::load()
        .map(|config| config.effective_mode() == ConfigMode::Local)
        .unwrap_or(false)
}

fn installed_server_version() -> Option<String> {
    let binary = find_on_path(SERVER_BINARY)?;
    let metadata_path = binary.parent()?.join(INSTALL_METADATA_FILE);
    read_metadata_value(&metadata_path, "server_version")
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn read_metadata_value(path: &Path, key: &str) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    for line in contents.lines() {
        let line = line.trim();
        let Some((line_key, value)) = line.split_once('=') else {
            continue;
        };
        if line_key.trim() == key {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn normalize_version(value: &str) -> &str {
    value.strip_prefix('v').unwrap_or(value)
}

fn parse_version(value: &str) -> Option<[u64; 3]> {
    let mut parts = normalize_version(value).split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;

    if parts.next().is_some() {
        return None;
    }

    Some([major, minor, patch])
}

fn is_newer_version(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        UpdateState, UpdateStatus, UpdateTarget, dismissed_matches, is_newer_version,
        parse_version, read_metadata_value,
    };
    use std::fs;

    #[test]
    fn parses_v_prefixed_semver() {
        assert_eq!(parse_version("v1.2.3"), Some([1, 2, 3]));
        assert_eq!(parse_version("1.2.3"), Some([1, 2, 3]));
    }

    #[test]
    fn rejects_non_semver() {
        assert_eq!(parse_version("v1.2"), None);
        assert_eq!(parse_version("latest"), None);
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer_version("v0.2.15", "0.2.14"));
        assert!(is_newer_version("v0.3.0", "0.2.14"));
        assert!(!is_newer_version("v0.2.14", "0.2.14"));
        assert!(!is_newer_version("v0.2.13", "0.2.14"));
    }

    #[test]
    fn reads_install_metadata_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".pulse-install.toml");
        fs::write(
            &path,
            "server_version = \"v0.2.12\"\ncli_version = \"v0.2.14\"\n",
        )
        .unwrap();

        assert_eq!(
            read_metadata_value(&path, "server_version").as_deref(),
            Some("v0.2.12")
        );
    }

    #[test]
    fn local_update_target_includes_server_release() {
        let status = UpdateStatus {
            latest_cli: "v0.2.15".to_string(),
            current_cli: "0.2.14",
            latest_server: "v0.2.13".to_string(),
            current_server: Some("v0.2.12".to_string()),
            local_managed: true,
        };

        assert_eq!(
            status.target(),
            UpdateTarget {
                latest_cli: "v0.2.15".to_string(),
                latest_server: Some("v0.2.13".to_string())
            }
        );
    }

    #[test]
    fn remote_update_target_tracks_cli_only() {
        let status = UpdateStatus {
            latest_cli: "v0.2.15".to_string(),
            current_cli: "0.2.14",
            latest_server: "v0.2.13".to_string(),
            current_server: None,
            local_managed: false,
        };

        assert_eq!(
            status.target(),
            UpdateTarget {
                latest_cli: "v0.2.15".to_string(),
                latest_server: None
            }
        );
    }

    #[test]
    fn dismissed_target_suppresses_same_update() {
        let status = UpdateStatus {
            latest_cli: "v0.2.15".to_string(),
            current_cli: "0.2.14",
            latest_server: "v0.2.13".to_string(),
            current_server: Some("v0.2.12".to_string()),
            local_managed: true,
        };
        let state = UpdateState {
            dismissed: Some(status.target()),
        };

        assert!(dismissed_matches(&state, &status));
    }

    #[test]
    fn newer_server_release_ignores_old_dismissal() {
        let old_status = UpdateStatus {
            latest_cli: "v0.2.15".to_string(),
            current_cli: "0.2.14",
            latest_server: "v0.2.13".to_string(),
            current_server: Some("v0.2.12".to_string()),
            local_managed: true,
        };
        let newer_status = UpdateStatus {
            latest_cli: "v0.2.15".to_string(),
            current_cli: "0.2.14",
            latest_server: "v0.2.14".to_string(),
            current_server: Some("v0.2.12".to_string()),
            local_managed: true,
        };
        let state = UpdateState {
            dismissed: Some(old_status.target()),
        };

        assert!(!dismissed_matches(&state, &newer_status));
    }
}
