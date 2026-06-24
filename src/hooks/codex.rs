use std::{fs, io::ErrorKind, path::PathBuf};

use dirs::home_dir;
use serde_json::{Map, Value, json};

use crate::error::{PulseError, Result};

use super::{HookStatus, ToolHook};

const CODEX_CONFIG_DIR: &str = ".codex";
const CODEX_HOOKS_FILE: &str = "hooks.json";
const CODEX_TOOL_NAME: &str = "Codex";
pub const CODEX_SOURCE: &str = "codex";

pub const HOOK_DEFINITIONS: &[(&str, &str, &str)] = &[
    (
        "SessionStart",
        "startup|resume|clear|compact",
        "session_start",
    ),
    ("UserPromptSubmit", "", "user_prompt_submit"),
    ("PreToolUse", "*", "pre_tool_use"),
    ("PermissionRequest", "*", "permission_request"),
    ("PostToolUse", "*", "post_tool_use"),
    ("SubagentStart", "*", "subagent_start"),
    ("SubagentStop", "*", "subagent_stop"),
    ("Stop", "", "stop"),
];

#[derive(Debug, Clone)]
pub struct CodexHook {
    config_dir: PathBuf,
    hooks_path: PathBuf,
    pulse_binary: PathBuf,
}

impl CodexHook {
    pub fn new() -> Result<Self> {
        let home = home_dir().ok_or(PulseError::HomeDirNotFound)?;
        let pulse_binary =
            std::env::current_exe().unwrap_or_else(|_| home.join(".local/bin/pulse"));
        let config_dir = home.join(CODEX_CONFIG_DIR);
        let hooks_path = config_dir.join(CODEX_HOOKS_FILE);
        Ok(Self {
            config_dir,
            hooks_path,
            pulse_binary,
        })
    }

    fn pulse_command(&self, event_type: &str) -> String {
        format!(
            "{} emit-codex {}",
            shell_quote(&self.pulse_binary.to_string_lossy()),
            event_type
        )
    }

    fn read_hooks(&self) -> Result<Option<Value>> {
        match fs::read_to_string(&self.hooks_path) {
            Ok(contents) => {
                let value: Value = serde_json::from_str(&contents)?;
                Ok(Some(value))
            }
            Err(err) => {
                if err.kind() == ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(err.into())
                }
            }
        }
    }

    fn write_hooks(&self, value: &Value) -> Result<()> {
        fs::create_dir_all(&self.config_dir)?;
        let body = serde_json::to_string_pretty(value)?;
        fs::write(&self.hooks_path, body)?;
        Ok(())
    }

    fn hooks_map<'a>(value: &'a mut Value) -> Result<&'a mut Map<String, Value>> {
        let obj = value
            .as_object_mut()
            .ok_or_else(|| PulseError::message("Codex hooks file must contain a JSON object"))?;
        let hooks_value = obj
            .entry("hooks")
            .or_insert_with(|| Value::Object(Map::new()));
        hooks_value
            .as_object_mut()
            .ok_or_else(|| PulseError::message("`hooks` field must be a JSON object"))
    }

    fn ensure_command(events: &mut Vec<Value>, matcher: &str, command: &str) -> bool {
        let already_present = events
            .iter()
            .any(|entry| entry_contains_command(entry, command));
        if already_present {
            return false;
        }

        let mut entry = json!({
            "hooks": [{
                "type": "command",
                "command": command,
                "statusMessage": "Recording Pulse event"
            }]
        });
        if !matcher.is_empty() {
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("matcher".to_string(), Value::String(matcher.to_string()));
            }
        }

        events.push(entry);
        true
    }

    fn insert_hooks<F>(value: &mut Value, command_for_event: F) -> Result<bool>
    where
        F: Fn(&str) -> String,
    {
        let hooks_map = Self::hooks_map(value)?;
        let mut changed = false;
        for (event, matcher, event_type) in HOOK_DEFINITIONS {
            let command = command_for_event(event_type);
            let entry = hooks_map
                .entry((*event).to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            let events = entry
                .as_array_mut()
                .ok_or_else(|| PulseError::message("Hook event entries must be arrays"))?;
            if Self::ensure_command(events, matcher, &command) {
                changed = true;
            }
        }
        Ok(changed)
    }

    fn remove_hooks(value: &mut Value) -> Result<bool> {
        let hooks_map = match value
            .as_object_mut()
            .and_then(|obj| obj.get_mut("hooks"))
            .and_then(|hooks| hooks.as_object_mut())
        {
            Some(map) => map,
            None => return Ok(false),
        };

        let mut changed = false;
        let mut empty_events: Vec<String> = Vec::new();

        for (event, _, event_type) in HOOK_DEFINITIONS {
            if let Some(event_value) = hooks_map.get_mut(*event) {
                let array = event_value
                    .as_array_mut()
                    .ok_or_else(|| PulseError::message("Hook event entries must be arrays"))?;
                for entry in array.iter_mut() {
                    if remove_command(entry, event_type) {
                        changed = true;
                    }
                }
                array.retain(|entry| !entry_is_empty(entry));
                if array.is_empty() {
                    empty_events.push((*event).to_string());
                }
            }
        }

        for key in empty_events {
            hooks_map.remove(&key);
            changed = true;
        }

        if hooks_map.is_empty() {
            if let Some(obj) = value.as_object_mut() {
                obj.remove("hooks");
            }
            changed = true;
        }

        Ok(changed)
    }

    fn current_status(&self) -> Result<HookStatus> {
        if !self.config_dir.exists() {
            return Ok(HookStatus::not_detected(
                self.tool_name(),
                self.config_dir.clone(),
            ));
        }

        let value = self.read_hooks()?.unwrap_or(Value::Object(Map::new()));
        let (installed, total, names) = installed_hook_counts(&value);
        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected: installed == total,
            modified: false,
            path: Some(self.hooks_path.clone()),
            message: None,
            installed_hooks: installed,
            total_hooks: total,
            installed_hook_names: names,
        })
    }
}

impl ToolHook for CodexHook {
    fn tool_name(&self) -> &'static str {
        CODEX_TOOL_NAME
    }

    fn status(&self) -> Result<HookStatus> {
        self.current_status()
    }

    fn connect(&self) -> Result<HookStatus> {
        if !self.config_dir.exists() {
            return Ok(HookStatus::not_detected(
                self.tool_name(),
                self.config_dir.clone(),
            ));
        }

        let mut value = self.read_hooks()?.unwrap_or(Value::Object(Map::new()));
        let changed = Self::insert_hooks(&mut value, |event_type| self.pulse_command(event_type))?;
        if changed {
            self.write_hooks(&value)?;
        }
        let (installed, total, names) = installed_hook_counts(&value);
        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected: installed == total,
            modified: changed,
            path: Some(self.hooks_path.clone()),
            message: Some("Run `/hooks` in Codex to review and trust new hooks.".to_string()),
            installed_hooks: installed,
            total_hooks: total,
            installed_hook_names: names,
        })
    }

    fn disconnect(&self) -> Result<HookStatus> {
        if !self.config_dir.exists() {
            return Ok(HookStatus::not_detected(
                self.tool_name(),
                self.config_dir.clone(),
            ));
        }

        let mut value = self.read_hooks()?.unwrap_or(Value::Object(Map::new()));
        let changed = Self::remove_hooks(&mut value)?;
        if changed {
            self.write_hooks(&value)?;
        }
        let (installed, total, names) = installed_hook_counts(&value);
        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected: installed == total,
            modified: changed,
            path: Some(self.hooks_path.clone()),
            message: None,
            installed_hooks: installed,
            total_hooks: total,
            installed_hook_names: names,
        })
    }
}

fn installed_hook_counts(value: &Value) -> (usize, usize, Vec<String>) {
    let total = HOOK_DEFINITIONS.len();
    let hooks_map = match value
        .as_object()
        .and_then(|obj| obj.get("hooks"))
        .and_then(|hooks| hooks.as_object())
    {
        Some(map) => map,
        None => return (0, total, Vec::new()),
    };

    let mut names = Vec::new();
    for (event, _, event_type) in HOOK_DEFINITIONS {
        let present = hooks_map
            .get(*event)
            .and_then(|value| value.as_array())
            .map(|array| {
                array
                    .iter()
                    .any(|entry| entry_contains_event_command(entry, event_type))
            })
            .unwrap_or(false);
        if present {
            names.push((*event).to_string());
        }
    }

    (names.len(), total, names)
}

fn entry_contains_command(entry: &Value, command: &str) -> bool {
    entry
        .as_object()
        .and_then(|obj| obj.get("hooks"))
        .and_then(|hooks| hooks.as_array())
        .map(|hooks| {
            hooks.iter().any(|hook| {
                hook.as_object()
                    .and_then(|hook_obj| hook_obj.get("command"))
                    .and_then(|cmd| cmd.as_str())
                    .map(|value| value == command)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn entry_contains_event_command(entry: &Value, event_type: &str) -> bool {
    entry
        .as_object()
        .and_then(|obj| obj.get("hooks"))
        .and_then(|hooks| hooks.as_array())
        .map(|hooks| {
            hooks.iter().any(|hook| {
                hook.as_object()
                    .and_then(|hook_obj| hook_obj.get("command"))
                    .and_then(|cmd| cmd.as_str())
                    .map(|value| command_matches_event(value, event_type))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn remove_command(entry: &mut Value, event_type: &str) -> bool {
    let hooks = match entry
        .as_object_mut()
        .and_then(|obj| obj.get_mut("hooks"))
        .and_then(|hooks| hooks.as_array_mut())
    {
        Some(hooks) => hooks,
        None => return false,
    };
    let initial_len = hooks.len();
    hooks.retain(|hook| {
        hook.as_object()
            .and_then(|obj| obj.get("command"))
            .and_then(|cmd| cmd.as_str())
            .map(|value| !command_matches_event(value, event_type))
            .unwrap_or(true)
    });
    hooks.len() != initial_len
}

fn entry_is_empty(entry: &Value) -> bool {
    entry
        .as_object()
        .and_then(|obj| obj.get("hooks"))
        .and_then(|hooks| hooks.as_array())
        .map(|hooks| hooks.is_empty())
        .unwrap_or(true)
}

fn command_matches_event(command: &str, event_type: &str) -> bool {
    let trimmed = command.trim();
    let suffix = format!(" emit-codex {event_type}");
    trimmed == format!("pulse{suffix}")
        || trimmed.ends_with(&format!("/pulse{suffix}"))
        || trimmed.ends_with(&format!("/pulse'{suffix}"))
        || trimmed.ends_with(&format!("/pulse\"{suffix}"))
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_hook(tmp: &TempDir) -> CodexHook {
        let config_dir = tmp.path().join(CODEX_CONFIG_DIR);
        let hooks_path = config_dir.join(CODEX_HOOKS_FILE);
        CodexHook {
            config_dir,
            hooks_path,
            pulse_binary: PathBuf::from("/tmp/pulse-test/bin/pulse"),
        }
    }

    fn test_command(event_type: &str) -> String {
        format!("/tmp/pulse-test/bin/pulse emit-codex {event_type}")
    }

    #[test]
    fn test_hook_definitions_count() {
        assert_eq!(HOOK_DEFINITIONS.len(), 8);
    }

    #[test]
    fn test_insert_hooks_into_empty_file() {
        let mut value = json!({});
        let changed = CodexHook::insert_hooks(&mut value, test_command).unwrap();
        assert!(changed);

        let (installed, total, names) = installed_hook_counts(&value);
        assert_eq!(installed, 8);
        assert_eq!(total, 8);
        assert_eq!(names.len(), 8);
    }

    #[test]
    fn test_insert_hooks_is_idempotent() {
        let mut value = json!({});
        CodexHook::insert_hooks(&mut value, test_command).unwrap();
        let changed = CodexHook::insert_hooks(&mut value, test_command).unwrap();
        assert!(!changed);
    }

    #[test]
    fn test_insert_preserves_existing_hooks() {
        let mut value = json!({
            "hooks": {
                "PostToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{"type": "command", "command": "other-tool do something"}]
                }]
            }
        });
        CodexHook::insert_hooks(&mut value, test_command).unwrap();
        let post_tool = value["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(post_tool.len(), 2);
    }

    #[test]
    fn test_remove_hooks_cleans_up() {
        let mut value = json!({});
        CodexHook::insert_hooks(&mut value, test_command).unwrap();
        let changed = CodexHook::remove_hooks(&mut value).unwrap();
        assert!(changed);

        let (installed, _, _) = installed_hook_counts(&value);
        assert_eq!(installed, 0);
    }

    #[test]
    fn test_remove_only_removes_pulse_hooks() {
        let mut value = json!({
            "hooks": {
                "PostToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{"type": "command", "command": "other-tool do something"}]
                }]
            }
        });
        CodexHook::insert_hooks(&mut value, test_command).unwrap();
        CodexHook::remove_hooks(&mut value).unwrap();

        let post_tool = value["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(post_tool.len(), 1);
        assert!(entry_contains_command(
            &post_tool[0],
            "other-tool do something"
        ));
    }

    #[test]
    fn test_status_counts_legacy_bare_pulse_commands() {
        let mut value = json!({
            "hooks": {
                "PostToolUse": [{
                    "matcher": "*",
                    "hooks": [{"type": "command", "command": "pulse emit-codex post_tool_use"}]
                }]
            }
        });

        let (installed, _, names) = installed_hook_counts(&value);
        assert_eq!(installed, 1);
        assert_eq!(names, vec!["PostToolUse"]);

        let changed = CodexHook::remove_hooks(&mut value).unwrap();
        assert!(changed);
        let (installed, _, _) = installed_hook_counts(&value);
        assert_eq!(installed, 0);
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
    fn test_detected_but_not_connected_when_config_dir_exists() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        let status = hook.status().unwrap();
        assert!(status.detected);
        assert!(!status.connected);
        assert_eq!(status.installed_hooks, 0);
        assert_eq!(status.total_hooks, 8);
    }

    #[test]
    fn test_connect_creates_hooks_json_when_codex_dir_exists() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        let status = hook.connect().unwrap();
        assert!(status.detected);
        assert!(status.connected);
        assert!(status.modified);
        assert_eq!(status.installed_hooks, 8);
        assert!(hook.hooks_path.exists());
    }

    #[test]
    fn test_connect_writes_absolute_pulse_commands() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        hook.connect().unwrap();
        let value: Value =
            serde_json::from_str(&fs::read_to_string(&hook.hooks_path).unwrap()).unwrap();
        let session_start = value["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();

        assert_eq!(
            session_start,
            "/tmp/pulse-test/bin/pulse emit-codex session_start"
        );
    }

    #[test]
    fn test_connect_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        hook.connect().unwrap();
        let status = hook.connect().unwrap();
        assert!(!status.modified);
        assert!(status.connected);
    }

    #[test]
    fn test_disconnect_removes_only_pulse_entries() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(&hook.config_dir).unwrap();

        hook.connect().unwrap();
        let status = hook.disconnect().unwrap();
        assert!(status.modified);
        assert!(!status.connected);
    }
}
