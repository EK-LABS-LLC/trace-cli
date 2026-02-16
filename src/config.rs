use std::{fs, io::ErrorKind, path::PathBuf};

use dirs::home_dir;
use serde::{Deserialize, Serialize};

use crate::error::{PulseError, Result};

const CONFIG_DIR: &str = ".pulse";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PulseConfig {
    pub api_url: String,
    pub api_key: String,
    pub project_id: String,
}

impl PulseConfig {
    pub fn sanitized(mut self) -> Self {
        self.api_url = self.api_url.trim_end_matches('/').trim().to_string();
        self.api_key = self.api_key.trim().to_string();
        self.project_id = self.project_id.trim().to_string();
        self
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
