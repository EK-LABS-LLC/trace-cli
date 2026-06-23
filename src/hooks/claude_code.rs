use std::{fs, io::ErrorKind, path::PathBuf};

use dirs::home_dir;
use serde_json::{Map, Value, json};

use crate::error::{PulseError, Result};

use super::{HookStatus, ToolHook};

const CLAUDE_SETTINGS: &str = ".claude/settings.json";
const CLAUDE_SETTINGS_LOCAL: &str = ".claude/settings.local.json";
const CLAUDE_TOOL_NAME: &str = "Claude Code";
pub const CLAUDE_SOURCE: &str = "claude_code";
pub const HOOK_DEFINITIONS: &[(&str, &str)] = &[
    ("PreToolUse", "pre_tool_use"),
    ("PostToolUse", "post_tool_use"),
    ("PostToolUseFailure", "post_tool_use_failure"),
    ("SessionStart", "session_start"),
    ("SessionEnd", "session_end"),
    ("Stop", "stop"),
    ("SubagentStart", "subagent_start"),
    ("SubagentStop", "subagent_stop"),
    ("UserPromptSubmit", "user_prompt_submit"),
    ("Notification", "notification"),
];

#[derive(Debug, Clone)]
pub struct ClaudeCodeHook {
    settings_path: PathBuf,
    legacy_local_path: PathBuf,
    pulse_binary: PathBuf,
}

impl ClaudeCodeHook {
    pub fn new() -> Result<Self> {
        let home = home_dir().ok_or(PulseError::HomeDirNotFound)?;
        let pulse_binary =
            std::env::current_exe().unwrap_or_else(|_| home.join(".local/bin/pulse"));
        Ok(Self {
            settings_path: home.join(CLAUDE_SETTINGS),
            legacy_local_path: home.join(CLAUDE_SETTINGS_LOCAL),
            pulse_binary,
        })
    }

    fn is_detected(&self) -> bool {
        self.settings_path
            .parent()
            .map(|path| path.exists())
            .unwrap_or(false)
    }

    fn read_settings_path(path: &PathBuf) -> Result<Option<Value>> {
        match fs::read_to_string(path) {
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

    fn read_settings(&self) -> Result<Option<Value>> {
        Self::read_settings_path(&self.settings_path)
    }

    fn write_settings_path(path: &PathBuf, value: &Value) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(value)?;
        fs::write(path, body)?;
        Ok(())
    }

    fn write_settings(&self, value: &Value) -> Result<()> {
        Self::write_settings_path(&self.settings_path, value)
    }

    fn pulse_command(&self, event_type: &str) -> String {
        format!(
            "{} emit {}",
            shell_quote(&self.pulse_binary.to_string_lossy()),
            event_type
        )
    }

    fn cleanup_legacy_local_hooks(&self) -> Result<bool> {
        if !self.legacy_local_path.exists() {
            return Ok(false);
        }
        let Some(mut value) = Self::read_settings_path(&self.legacy_local_path)? else {
            return Ok(false);
        };
        let changed = Self::remove_hooks(&mut value)?;
        if changed {
            Self::write_settings_path(&self.legacy_local_path, &value)?;
        }
        Ok(changed)
    }

    fn migrate_legacy_local_hooks(&self, target: &mut Value) -> Result<bool> {
        if !self.legacy_local_path.exists() {
            return Ok(false);
        }
        let Some(mut legacy) = Self::read_settings_path(&self.legacy_local_path)? else {
            return Ok(false);
        };
        let had_legacy_hooks = installed_hook_counts(&legacy).0 > 0;
        if had_legacy_hooks {
            Self::remove_hooks(&mut legacy)?;
            Self::write_settings_path(&self.legacy_local_path, &legacy)?;
            Self::insert_hooks(target, |event_type| self.pulse_command(event_type))?;
        }
        Ok(had_legacy_hooks)
    }

    fn any_settings_exist(&self) -> bool {
        self.settings_path.exists() || self.legacy_local_path.exists() || self.is_detected()
    }

    fn read_effective_settings(&self) -> Result<Option<Value>> {
        if self.settings_path.exists() {
            return self.read_settings();
        }
        if self.legacy_local_path.exists() {
            return Self::read_settings_path(&self.legacy_local_path);
        }
        Ok(None)
    }

    fn effective_status_path(&self) -> PathBuf {
        if self.settings_path.exists() {
            self.settings_path.clone()
        } else if self.legacy_local_path.exists() {
            self.legacy_local_path.clone()
        } else {
            self.settings_path.clone()
        }
    }

    fn settings_for_connect(&self) -> Result<Value> {
        if let Some(value) = self.read_settings()? {
            return Ok(value);
        }
        Ok(Value::Object(Map::new()))
    }

    fn target_not_detected(&self) -> HookStatus {
        HookStatus::not_detected(self.tool_name(), self.settings_path.clone())
    }

    fn status_from_value(&self, value: &Value, modified: bool, path: PathBuf) -> HookStatus {
        let (installed, total, names) = installed_hook_counts(value);
        HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected: installed == total,
            modified,
            path: Some(path),
            message: None,
            installed_hooks: installed,
            total_hooks: total,
            installed_hook_names: names,
        }
    }

    fn ensure_target_settings_file(&self) -> Result<()> {
        if self.settings_path.exists() {
            return Ok(());
        }
        self.write_settings(&Value::Object(Map::new()))?;
        Ok(())
    }

    fn hooks_map<'a>(value: &'a mut Value) -> Result<&'a mut Map<String, Value>> {
        let obj = value.as_object_mut().ok_or_else(|| {
            PulseError::message("Claude settings file must contain a JSON object")
        })?;
        let hooks_value = obj
            .entry("hooks")
            .or_insert_with(|| Value::Object(Map::new()));
        hooks_value
            .as_object_mut()
            .ok_or_else(|| PulseError::message("`hooks` field must be a JSON object"))
    }

    fn ensure_command(events: &mut Vec<Value>, event_type: &str, command: &str) -> bool {
        let mut changed = false;
        let mut found = false;
        for entry in events.iter_mut() {
            let hooks = entry
                .as_object_mut()
                .and_then(|obj| obj.get_mut("hooks"))
                .and_then(|hooks| hooks.as_array_mut());
            let Some(hooks) = hooks else {
                continue;
            };
            for hook in hooks {
                let hook_obj = hook.as_object_mut();
                let Some(hook_obj) = hook_obj else {
                    continue;
                };
                let existing = hook_obj.get("command").and_then(|cmd| cmd.as_str());
                if existing.is_some_and(|value| command_matches_event(value, event_type)) {
                    found = true;
                    if existing != Some(command) {
                        hook_obj.insert("command".to_string(), Value::String(command.to_string()));
                        changed = true;
                    }
                }
            }
        }
        if found {
            return changed;
        }

        let hook_value = json!({
            "matcher": "",
            "hooks": [{
                "type": "command",
                "command": command,
                "async": true
            }]
        });
        events.push(hook_value);
        true
    }

    fn insert_hooks<F>(value: &mut Value, command_for: F) -> Result<bool>
    where
        F: Fn(&str) -> String,
    {
        let hooks_map = Self::hooks_map(value)?;
        let mut changed = false;
        for (event, event_type) in HOOK_DEFINITIONS {
            let entry = hooks_map
                .entry((*event).to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            let events = entry
                .as_array_mut()
                .ok_or_else(|| PulseError::message("Hook event entries must be arrays"))?;
            let command = command_for(event_type);
            if Self::ensure_command(events, event_type, &command) {
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

        for (event, event_type) in HOOK_DEFINITIONS {
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
        if !self.any_settings_exist() {
            return Ok(self.target_not_detected());
        }
        let Some(value) = self.read_effective_settings()? else {
            return Ok(self.target_not_detected());
        };
        Ok(self.status_from_value(&value, false, self.effective_status_path()))
    }
}

impl ToolHook for ClaudeCodeHook {
    fn tool_name(&self) -> &'static str {
        CLAUDE_TOOL_NAME
    }

    fn status(&self) -> Result<HookStatus> {
        self.current_status()
    }

    fn connect(&self) -> Result<HookStatus> {
        if !self.is_detected() {
            return Ok(self.target_not_detected());
        }
        self.ensure_target_settings_file()?;
        let mut value = self.settings_for_connect()?;
        let migrated = self.migrate_legacy_local_hooks(&mut value)?;
        let changed = Self::insert_hooks(&mut value, |event_type| self.pulse_command(event_type))?
            || migrated;
        if changed {
            self.write_settings(&value)?;
        }
        Ok(self.status_from_value(&value, changed, self.settings_path.clone()))
    }

    fn disconnect(&self) -> Result<HookStatus> {
        if !self.any_settings_exist() {
            return Ok(self.target_not_detected());
        }
        let mut value = match self.read_settings()? {
            Some(value) => value,
            None => Value::Object(Map::new()),
        };
        let target_changed = Self::remove_hooks(&mut value)?;
        let legacy_changed = self.cleanup_legacy_local_hooks()?;
        let changed = target_changed || legacy_changed;
        if changed {
            self.write_settings(&value)?;
        }
        Ok(self.status_from_value(&value, changed, self.settings_path.clone()))
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
    for (event, event_type) in HOOK_DEFINITIONS {
        let present = hooks_map
            .get(*event)
            .and_then(|value| value.as_array())
            .map(|array| {
                array
                    .iter()
                    .any(|entry| entry_contains_command(entry, event_type))
            })
            .unwrap_or(false);
        if present {
            names.push((*event).to_string());
        }
    }

    let installed = names.len();
    (installed, total, names)
}

fn entry_contains_command(entry: &Value, event_type: &str) -> bool {
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
    let suffix = format!(" emit {event_type}");
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
    use tempfile::TempDir;

    fn test_command(event_type: &str) -> String {
        format!("/tmp/pulse-test/bin/pulse emit {event_type}")
    }

    fn make_hook(tmp: &TempDir) -> ClaudeCodeHook {
        let settings_path = tmp.path().join(CLAUDE_SETTINGS);
        let legacy_local_path = tmp.path().join(CLAUDE_SETTINGS_LOCAL);
        ClaudeCodeHook {
            settings_path,
            legacy_local_path,
            pulse_binary: PathBuf::from("/tmp/pulse-test/bin/pulse"),
        }
    }

    #[test]
    fn test_hook_definitions_count() {
        assert_eq!(HOOK_DEFINITIONS.len(), 10);
    }

    #[test]
    fn test_hook_definitions_all_unique_events() {
        let events: Vec<&str> = HOOK_DEFINITIONS.iter().map(|(e, _)| *e).collect();
        let mut deduped = events.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(events.len(), deduped.len(), "duplicate event names found");
    }

    #[test]
    fn test_hook_definitions_all_unique_commands() {
        let cmds: Vec<&str> = HOOK_DEFINITIONS.iter().map(|(_, c)| *c).collect();
        let mut deduped = cmds.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(cmds.len(), deduped.len(), "duplicate event types found");
    }

    #[test]
    fn test_insert_hooks_into_empty_settings() {
        let mut value = json!({});
        let changed = ClaudeCodeHook::insert_hooks(&mut value, test_command).unwrap();
        assert!(changed);

        let (installed, total, names) = installed_hook_counts(&value);
        assert_eq!(installed, 10);
        assert_eq!(total, 10);
        assert_eq!(names.len(), 10);
    }

    #[test]
    fn test_insert_hooks_is_idempotent() {
        let mut value = json!({});
        ClaudeCodeHook::insert_hooks(&mut value, test_command).unwrap();
        let changed = ClaudeCodeHook::insert_hooks(&mut value, test_command).unwrap();
        assert!(!changed, "second insert should not change anything");
    }

    #[test]
    fn test_remove_hooks_cleans_up() {
        let mut value = json!({});
        ClaudeCodeHook::insert_hooks(&mut value, test_command).unwrap();
        let changed = ClaudeCodeHook::remove_hooks(&mut value).unwrap();
        assert!(changed);

        let (installed, _, _) = installed_hook_counts(&value);
        assert_eq!(installed, 0);
    }

    #[test]
    fn test_remove_hooks_on_empty_is_noop() {
        let mut value = json!({});
        let changed = ClaudeCodeHook::remove_hooks(&mut value).unwrap();
        assert!(!changed);
    }

    #[test]
    fn test_insert_preserves_existing_hooks() {
        let mut value = json!({
            "hooks": {
                "PostToolUse": [{
                    "matcher": "",
                    "hooks": [{"type": "command", "command": "other-tool do something"}]
                }]
            }
        });
        ClaudeCodeHook::insert_hooks(&mut value, test_command).unwrap();

        // The existing hook entry should still be there
        let post_tool = value["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(post_tool.len(), 2, "should have original + pulse hook");
    }

    #[test]
    fn test_remove_only_removes_pulse_hooks() {
        let mut value = json!({
            "hooks": {
                "PostToolUse": [{
                    "matcher": "",
                    "hooks": [{"type": "command", "command": "other-tool do something"}]
                }]
            }
        });
        ClaudeCodeHook::insert_hooks(&mut value, test_command).unwrap();
        ClaudeCodeHook::remove_hooks(&mut value).unwrap();

        // The non-pulse hook should remain
        let post_tool = value["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(post_tool.len(), 1);
        assert_eq!(
            post_tool[0]["hooks"][0]["command"].as_str().unwrap(),
            "other-tool do something"
        );
    }

    #[test]
    fn test_installed_hook_counts_partial() {
        // Simulate an old install with only 3 hooks
        let mut value = json!({});
        ClaudeCodeHook::insert_hooks(&mut value, test_command).unwrap();

        // Remove some hooks manually
        let hooks_map = value["hooks"].as_object_mut().unwrap();
        hooks_map.remove("PreToolUse");
        hooks_map.remove("SubagentStart");
        hooks_map.remove("SubagentStop");

        let (installed, total, names) = installed_hook_counts(&value);
        assert_eq!(total, 10);
        assert_eq!(installed, 7);
        assert_eq!(names.len(), 7);
        assert!(!names.contains(&"PreToolUse".to_string()));
        assert!(!names.contains(&"SubagentStart".to_string()));
    }

    #[test]
    fn test_updates_bare_pulse_commands_to_absolute_path() {
        let mut value = json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "",
                    "hooks": [{
                        "type": "command",
                        "command": "pulse emit session_start",
                        "async": true
                    }]
                }]
            }
        });

        let changed = ClaudeCodeHook::insert_hooks(&mut value, test_command).unwrap();
        assert!(changed);
        let hooks = value["hooks"]["SessionStart"][0]["hooks"]
            .as_array()
            .unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(
            hooks[0]["command"].as_str().unwrap(),
            "/tmp/pulse-test/bin/pulse emit session_start"
        );
    }

    #[test]
    fn test_connect_installs_to_user_settings_json() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(hook.settings_path.parent().unwrap()).unwrap();

        let status = hook.connect().unwrap();
        assert!(status.detected);
        assert!(status.connected);
        assert!(status.modified);
        assert!(hook.settings_path.exists());
        assert!(!hook.legacy_local_path.exists());
    }

    #[test]
    fn test_connect_migrates_legacy_local_hooks() {
        let tmp = TempDir::new().unwrap();
        let hook = make_hook(&tmp);
        fs::create_dir_all(hook.legacy_local_path.parent().unwrap()).unwrap();

        let mut legacy = json!({});
        ClaudeCodeHook::insert_hooks(&mut legacy, |event_type| format!("pulse emit {event_type}"))
            .unwrap();
        ClaudeCodeHook::write_settings_path(&hook.legacy_local_path, &legacy).unwrap();

        let status = hook.connect().unwrap();
        assert!(status.connected);
        assert!(hook.settings_path.exists());

        let user = ClaudeCodeHook::read_settings_path(&hook.settings_path)
            .unwrap()
            .unwrap();
        assert_eq!(installed_hook_counts(&user).0, 10);

        let legacy = ClaudeCodeHook::read_settings_path(&hook.legacy_local_path)
            .unwrap()
            .unwrap();
        assert_eq!(installed_hook_counts(&legacy).0, 0);
    }
}
