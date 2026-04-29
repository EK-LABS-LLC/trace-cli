use std::{fs, io::ErrorKind, path::PathBuf};

use dirs::home_dir;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::error::{PulseError, Result};

const CONFIG_DIR: &str = ".pulse";
const CONFIG_FILE: &str = "config.toml";
const RUN_DIR: &str = "run";
const LOG_DIR: &str = "logs";
const SERVER_PID_FILE: &str = "server.pid";
const SERVER_LOG_FILE: &str = "server.log";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConfigMode {
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PulseConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ConfigMode>,
    pub api_url: String,
    pub api_key: String,
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_password: Option<String>,
}

impl PulseConfig {
    pub fn effective_mode(&self) -> ConfigMode {
        self.mode.unwrap_or_else(|| infer_mode(self))
    }

    pub fn sanitized(mut self) -> Self {
        self.api_url = self.api_url.trim_end_matches('/').trim().to_string();
        self.api_key = self.api_key.trim().to_string();
        self.project_id = self.project_id.trim().to_string();
        self.server_command = self
            .server_command
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.local_email = self
            .local_email
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.local_password = self
            .local_password
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.mode = Some(self.effective_mode());
        self
    }
}

fn infer_mode(config: &PulseConfig) -> ConfigMode {
    let has_local_creds = config.local_email.is_some() && config.local_password.is_some();
    if has_local_creds && is_loopback_api_url(&config.api_url) {
        ConfigMode::Local
    } else {
        ConfigMode::Remote
    }
}

fn is_loopback_api_url(raw: &str) -> bool {
    let trimmed = raw.trim().trim_end_matches('/');
    match Url::parse(trimmed) {
        Ok(url) => matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "::1" | "[::1]")
        ),
        Err(_) => false,
    }
}

pub struct ConfigStore;

impl ConfigStore {
    pub fn config_dir() -> Result<PathBuf> {
        let home = home_dir().ok_or(PulseError::HomeDirNotFound)?;
        Ok(home.join(CONFIG_DIR))
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join(CONFIG_FILE))
    }

    pub fn run_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join(RUN_DIR))
    }

    pub fn log_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join(LOG_DIR))
    }

    pub fn server_pid_path() -> Result<PathBuf> {
        Ok(Self::run_dir()?.join(SERVER_PID_FILE))
    }

    pub fn server_log_path() -> Result<PathBuf> {
        Ok(Self::log_dir()?.join(SERVER_LOG_FILE))
    }

    pub fn load() -> Result<PulseConfig> {
        let path = Self::config_path()?;
        let contents = fs::read_to_string(path).map_err(|err| {
            if err.kind() == ErrorKind::NotFound {
                PulseError::ConfigMissing
            } else {
                err.into()
            }
        })?;
        let config: PulseConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn save(config: &PulseConfig) -> Result<()> {
        let dir = Self::config_dir()?;
        fs::create_dir_all(&dir)?;
        let body = toml::to_string_pretty(config)?;
        fs::write(dir.join(CONFIG_FILE), body)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfigMode, PulseConfig};

    fn base_config() -> PulseConfig {
        PulseConfig {
            mode: None,
            api_url: "http://localhost:3000".to_string(),
            api_key: "pulse_sk_test".to_string(),
            project_id: "project_test".to_string(),
            server_command: None,
            local_email: None,
            local_password: None,
        }
    }

    #[test]
    fn infers_local_mode_when_local_credentials_exist() {
        let mut config = base_config();
        config.local_email = Some("local@pulse.test".to_string());
        config.local_password = Some("secret".to_string());

        assert_eq!(config.effective_mode(), ConfigMode::Local);
    }

    #[test]
    fn infers_remote_mode_without_local_credentials() {
        let config = base_config();
        assert_eq!(config.effective_mode(), ConfigMode::Remote);
    }

    #[test]
    fn sanitize_persists_effective_mode() {
        let mut config = base_config();
        config.local_email = Some(" local@pulse.test ".to_string());
        config.local_password = Some(" secret ".to_string());

        let sanitized = config.sanitized();
        assert_eq!(sanitized.mode, Some(ConfigMode::Local));
        assert_eq!(sanitized.local_email.as_deref(), Some("local@pulse.test"));
        assert_eq!(sanitized.local_password.as_deref(), Some("secret"));
    }
}
