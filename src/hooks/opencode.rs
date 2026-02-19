use std::{fs, path::PathBuf};

use dirs::home_dir;

use crate::error::{PulseError, Result};

use super::{HookStatus, ToolHook};

const OPENCODE_CONFIG_DIR: &str = ".config/opencode";
const OPENCODE_PLUGIN_FILENAME: &str = "pulse-plugin.ts";
const OPENCODE_TOOL_NAME: &str = "OpenCode";
const PLUGIN_SOURCE: &str = include_str!("../../plugins/opencode/pulse-plugin.ts");

#[derive(Debug, Clone)]
pub struct OpenCodeHook {
    config_dir: PathBuf,
    plugin_path: PathBuf,
}

impl OpenCodeHook {
    pub fn new() -> Result<Self> {
        let home = home_dir().ok_or(PulseError::HomeDirNotFound)?;
        let config_dir = home.join(OPENCODE_CONFIG_DIR);
        let plugin_path = config_dir.join("plugins").join(OPENCODE_PLUGIN_FILENAME);
        Ok(Self {
            config_dir,
            plugin_path,
        })
    }

    fn is_detected(&self) -> bool {
        self.config_dir.exists()
    }

    fn plugin_installed(&self) -> bool {
        self.plugin_path.exists()
    }

    fn plugin_matches(&self) -> bool {
        match fs::read_to_string(&self.plugin_path) {
            Ok(contents) => contents == PLUGIN_SOURCE,
            Err(_) => false,
        }
    }
}

impl ToolHook for OpenCodeHook {
    fn tool_name(&self) -> &'static str {
        OPENCODE_TOOL_NAME
    }

    fn status(&self) -> Result<HookStatus> {
        if !self.is_detected() {
            return Ok(HookStatus::not_detected(
                self.tool_name(),
                self.config_dir.clone(),
            ));
        }

        let installed = self.plugin_installed();
        let up_to_date = installed && self.plugin_matches();

        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected: installed,
            modified: false,
            path: Some(self.plugin_path.clone()),
            message: if installed && !up_to_date {
                Some("Plugin installed but outdated".to_string())
            } else {
                None
            },
            installed_hooks: if installed { 1 } else { 0 },
            total_hooks: 1,
            installed_hook_names: if installed {
                vec!["pulse-plugin".to_string()]
            } else {
                Vec::new()
            },
        })
    }

    fn connect(&self) -> Result<HookStatus> {
        if !self.is_detected() {
            return Ok(HookStatus::not_detected(
                self.tool_name(),
                self.config_dir.clone(),
            ));
        }

        let already_current = self.plugin_installed() && self.plugin_matches();

        if !already_current {
            if let Some(parent) = self.plugin_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&self.plugin_path, PLUGIN_SOURCE)?;
        }

        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected: true,
            modified: !already_current,
            path: Some(self.plugin_path.clone()),
            message: None,
            installed_hooks: 1,
            total_hooks: 1,
            installed_hook_names: vec!["pulse-plugin".to_string()],
        })
    }

    fn disconnect(&self) -> Result<HookStatus> {
        if !self.is_detected() {
            return Ok(HookStatus::not_detected(
                self.tool_name(),
                self.config_dir.clone(),
            ));
        }

        let was_installed = self.plugin_installed();
        if was_installed {
            fs::remove_file(&self.plugin_path)?;
        }

        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected: false,
            modified: was_installed,
            path: Some(self.plugin_path.clone()),
            message: None,
            installed_hooks: 0,
            total_hooks: 1,
            installed_hook_names: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_hook(tmp: &TempDir) -> OpenCodeHook {
        let config_dir = tmp.path().join(".config/opencode");
        let plugin_path = config_dir.join("plugins").join(OPENCODE_PLUGIN_FILENAME);
        OpenCodeHook {
            config_dir,
            plugin_path,
        }
    }

    #[test]
    fn test_not_detected_when_config_dir_missing() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        let status = hook.status().unwrap();
        assert!(!status.detected);
        assert!(!status.connected);
    }

    #[test]
    fn test_detected_but_not_connected() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        let status = hook.status().unwrap();
        assert!(status.detected);
        assert!(!status.connected);
        assert_eq!(status.installed_hooks, 0);
    }

    #[test]
    fn test_connect_installs_plugin() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        let status = hook.connect().unwrap();
        assert!(status.detected);
        assert!(status.connected);
        assert!(status.modified);
        assert_eq!(status.installed_hooks, 1);
        assert!(hook.plugin_path.exists());

        let contents = fs::read_to_string(&hook.plugin_path).unwrap();
        assert_eq!(contents, PLUGIN_SOURCE);
    }

    #[test]
    fn test_connect_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        hook.connect().unwrap();
        let status = hook.connect().unwrap();
        assert!(!status.modified, "second connect should not modify");
        assert!(status.connected);
    }

    #[test]
    fn test_disconnect_removes_plugin() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        hook.connect().unwrap();
        let status = hook.disconnect().unwrap();
        assert!(status.modified);
        assert!(!status.connected);
        assert!(!hook.plugin_path.exists());
    }

    #[test]
    fn test_disconnect_noop_when_not_installed() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        let status = hook.disconnect().unwrap();
        assert!(!status.modified);
        assert!(!status.connected);
    }

    #[test]
    fn test_connect_updates_outdated_plugin() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        // Write an outdated plugin
        fs::create_dir_all(hook.plugin_path.parent().unwrap()).unwrap();
        fs::write(&hook.plugin_path, "// old version").unwrap();

        let status = hook.connect().unwrap();
        assert!(status.modified, "should update outdated plugin");

        let contents = fs::read_to_string(&hook.plugin_path).unwrap();
        assert_eq!(contents, PLUGIN_SOURCE);
    }
}
