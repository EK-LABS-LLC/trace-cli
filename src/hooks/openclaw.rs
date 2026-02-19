use std::{fs, path::PathBuf};

use dirs::home_dir;

use crate::error::{PulseError, Result};

use super::{HookStatus, ToolHook};

const OPENCLAW_CONFIG_DIR: &str = ".openclaw";
const OPENCLAW_HOOK_DIR: &str = "pulse-hook";
const OPENCLAW_TOOL_NAME: &str = "OpenClaw";

const HOOK_MD_SOURCE: &str = include_str!("../../plugins/openclaw/HOOK.md");
const HANDLER_TS_SOURCE: &str = include_str!("../../plugins/openclaw/handler.ts");

#[derive(Debug, Clone)]
pub struct OpenClawHook {
    config_dir: PathBuf,
    hook_dir: PathBuf,
    hook_md_path: PathBuf,
    handler_ts_path: PathBuf,
}

impl OpenClawHook {
    pub fn new() -> Result<Self> {
        let home = home_dir().ok_or(PulseError::HomeDirNotFound)?;
        let config_dir = home.join(OPENCLAW_CONFIG_DIR);
        let hook_dir = config_dir.join("hooks").join(OPENCLAW_HOOK_DIR);
        let hook_md_path = hook_dir.join("HOOK.md");
        let handler_ts_path = hook_dir.join("handler.ts");
        Ok(Self {
            config_dir,
            hook_dir,
            hook_md_path,
            handler_ts_path,
        })
    }

    fn is_detected(&self) -> bool {
        self.config_dir.exists()
    }

    fn files_installed(&self) -> bool {
        self.hook_md_path.exists() && self.handler_ts_path.exists()
    }

    fn files_match(&self) -> bool {
        let md_ok = fs::read_to_string(&self.hook_md_path)
            .map(|c| c == HOOK_MD_SOURCE)
            .unwrap_or(false);
        let ts_ok = fs::read_to_string(&self.handler_ts_path)
            .map(|c| c == HANDLER_TS_SOURCE)
            .unwrap_or(false);
        md_ok && ts_ok
    }
}

impl ToolHook for OpenClawHook {
    fn tool_name(&self) -> &'static str {
        OPENCLAW_TOOL_NAME
    }

    fn status(&self) -> Result<HookStatus> {
        if !self.is_detected() {
            return Ok(HookStatus::not_detected(
                self.tool_name(),
                self.config_dir.clone(),
            ));
        }

        let installed = self.files_installed();
        let up_to_date = installed && self.files_match();

        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected: installed,
            modified: false,
            path: Some(self.hook_dir.clone()),
            message: if installed && !up_to_date {
                Some("Hook installed but outdated".to_string())
            } else {
                None
            },
            installed_hooks: if installed { 1 } else { 0 },
            total_hooks: 1,
            installed_hook_names: if installed {
                vec!["pulse-hook".to_string()]
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

        let already_current = self.files_installed() && self.files_match();

        if !already_current {
            fs::create_dir_all(&self.hook_dir)?;
            fs::write(&self.hook_md_path, HOOK_MD_SOURCE)?;
            fs::write(&self.handler_ts_path, HANDLER_TS_SOURCE)?;
        }

        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected: true,
            modified: !already_current,
            path: Some(self.hook_dir.clone()),
            message: None,
            installed_hooks: 1,
            total_hooks: 1,
            installed_hook_names: vec!["pulse-hook".to_string()],
        })
    }

    fn disconnect(&self) -> Result<HookStatus> {
        if !self.is_detected() {
            return Ok(HookStatus::not_detected(
                self.tool_name(),
                self.config_dir.clone(),
            ));
        }

        let was_installed = self.files_installed();
        if was_installed {
            fs::remove_dir_all(&self.hook_dir)?;
        }

        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected: false,
            modified: was_installed,
            path: Some(self.hook_dir.clone()),
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

    fn make_hook(tmp: &TempDir) -> OpenClawHook {
        let config_dir = tmp.path().join(OPENCLAW_CONFIG_DIR);
        let hook_dir = config_dir.join("hooks").join(OPENCLAW_HOOK_DIR);
        let hook_md_path = hook_dir.join("HOOK.md");
        let handler_ts_path = hook_dir.join("handler.ts");
        OpenClawHook {
            config_dir,
            hook_dir,
            hook_md_path,
            handler_ts_path,
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
    fn test_connect_installs_hook() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        let status = hook.connect().unwrap();
        assert!(status.detected);
        assert!(status.connected);
        assert!(status.modified);
        assert_eq!(status.installed_hooks, 1);
        assert!(hook.hook_md_path.exists());
        assert!(hook.handler_ts_path.exists());

        let md = fs::read_to_string(&hook.hook_md_path).unwrap();
        assert_eq!(md, HOOK_MD_SOURCE);

        let ts = fs::read_to_string(&hook.handler_ts_path).unwrap();
        assert_eq!(ts, HANDLER_TS_SOURCE);
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
    fn test_disconnect_removes_hook_dir() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        hook.connect().unwrap();
        let status = hook.disconnect().unwrap();
        assert!(status.modified);
        assert!(!status.connected);
        assert!(!hook.hook_dir.exists());
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
    fn test_connect_updates_outdated_hook() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        // Write outdated files
        fs::create_dir_all(&hook.hook_dir).unwrap();
        fs::write(&hook.hook_md_path, "# old version").unwrap();
        fs::write(&hook.handler_ts_path, "// old version").unwrap();

        let status = hook.connect().unwrap();
        assert!(status.modified, "should update outdated hook");

        let md = fs::read_to_string(&hook.hook_md_path).unwrap();
        assert_eq!(md, HOOK_MD_SOURCE);
    }
}
