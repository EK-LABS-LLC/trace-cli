use std::{fs, io::ErrorKind, path::PathBuf};

use dirs::home_dir;
use serde_json::{Map, Value, json};

use crate::error::{PulseError, Result};

use super::{HookStatus, ToolHook};

const CLAUDE_SETTINGS: &str = ".claude/settings.json";
const CLAUDE_TOOL_NAME: &str = "Claude Code";
pub const CLAUDE_SOURCE: &str = "claude_code";
pub const HOOK_DEFINITIONS: &[(&str, &str)] = &[
    ("PreToolUse", "pulse emit pre_tool_use"),
    ("PostToolUse", "pulse emit post_tool_use"),
    ("PostToolUseFailure", "pulse emit post_tool_use_failure"),
    ("SessionStart", "pulse emit session_start"),
    ("SessionEnd", "pulse emit session_end"),
    ("Stop", "pulse emit stop"),
    ("SubagentStart", "pulse emit subagent_start"),
    ("SubagentStop", "pulse emit subagent_stop"),
    ("UserPromptSubmit", "pulse emit user_prompt_submit"),
    ("Notification", "pulse emit notification"),
];

#[derive(Debug, Clone)]
pub struct ClaudeCodeHook {
    settings_path: PathBuf,
}

impl ClaudeCodeHook {
    pub fn new() -> Result<Self> {
        let home = home_dir().ok_or(PulseError::HomeDirNotFound)?;
        Ok(Self {
            settings_path: home.join(CLAUDE_SETTINGS),
        })
    }

    fn read_settings(&self) -> Result<Option<Value>> {
        match fs::read_to_string(&self.settings_path) {
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

    fn write_settings(&self, value: &Value) -> Result<()> {
        if let Some(parent) = self.settings_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(value)?;
        fs::write(&self.settings_path, body)?;
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

    fn ensure_command(events: &mut Vec<Value>, command: &str) -> bool {
        let already_present = events
            .iter()
            .any(|entry| entry_contains_command(entry, command));
        if already_present {
            return false;
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

    fn insert_hooks(value: &mut Value) -> Result<bool> {
        let hooks_map = Self::hooks_map(value)?;
        let mut changed = false;
        for (event, command) in HOOK_DEFINITIONS {
            let entry = hooks_map
                .entry((*event).to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            let events = entry
                .as_array_mut()
                .ok_or_else(|| PulseError::message("Hook event entries must be arrays"))?;
            if Self::ensure_command(events, command) {
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

        for (event, command) in HOOK_DEFINITIONS {
            if let Some(event_value) = hooks_map.get_mut(*event) {
                let array = event_value
                    .as_array_mut()
                    .ok_or_else(|| PulseError::message("Hook event entries must be arrays"))?;
                for entry in array.iter_mut() {
                    if remove_command(entry, command) {
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
        if !self.settings_path.exists() {
            return Ok(HookStatus::not_detected(
                self.tool_name(),
                self.settings_path.clone(),
            ));
        }
        let Some(value) = self.read_settings()? else {
            return Ok(HookStatus::not_detected(
                self.tool_name(),
                self.settings_path.clone(),
            ));
        };
        let (installed, total, names) = installed_hook_counts(&value);
        let connected = installed == total;
        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected,
            modified: false,
            path: Some(self.settings_path.clone()),
            message: None,
            installed_hooks: installed,
            total_hooks: total,
            installed_hook_names: names,
        })
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
        if !self.settings_path.exists() {
            return Ok(HookStatus::not_detected(
                self.tool_name(),
                self.settings_path.clone(),
            ));
        }
        let mut value = self.read_settings()?.unwrap_or(Value::Object(Map::new()));
        let changed = Self::insert_hooks(&mut value)?;
        if changed {
            self.write_settings(&value)?;
        }
        let (installed, total, names) = installed_hook_counts(&value);
        let connected = installed == total;
        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected,
            modified: changed,
            path: Some(self.settings_path.clone()),
            message: None,
            installed_hooks: installed,
            total_hooks: total,
            installed_hook_names: names,
        })
    }

    fn disconnect(&self) -> Result<HookStatus> {
        if !self.settings_path.exists() {
            return Ok(HookStatus::not_detected(
                self.tool_name(),
                self.settings_path.clone(),
            ));
        }
        let mut value = match self.read_settings()? {
            Some(value) => value,
            None => Value::Object(Map::new()),
        };
        let changed = Self::remove_hooks(&mut value)?;
        if changed {
            self.write_settings(&value)?;
        }
        let (installed, total, names) = installed_hook_counts(&value);
        let connected = installed == total;
        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected,
            modified: changed,
            path: Some(self.settings_path.clone()),
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
    for (event, command) in HOOK_DEFINITIONS {
        let present = hooks_map
            .get(*event)
            .and_then(|value| value.as_array())
            .map(|array| {
                array
                    .iter()
                    .any(|entry| entry_contains_command(entry, command))
            })
            .unwrap_or(false);
        if present {
            names.push((*event).to_string());
        }
    }

    let installed = names.len();
    (installed, total, names)
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

fn remove_command(entry: &mut Value, command: &str) -> bool {
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
            .map(|value| value != command)
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(cmds.len(), deduped.len(), "duplicate commands found");
    }

    #[test]
    fn test_insert_hooks_into_empty_settings() {
        let mut value = json!({});
        let changed = ClaudeCodeHook::insert_hooks(&mut value).unwrap();
        assert!(changed);

        let (installed, total, names) = installed_hook_counts(&value);
        assert_eq!(installed, 10);
        assert_eq!(total, 10);
        assert_eq!(names.len(), 10);
    }

    #[test]
    fn test_insert_hooks_is_idempotent() {
        let mut value = json!({});
        ClaudeCodeHook::insert_hooks(&mut value).unwrap();
        let changed = ClaudeCodeHook::insert_hooks(&mut value).unwrap();
        assert!(!changed, "second insert should not change anything");
    }

    #[test]
    fn test_remove_hooks_cleans_up() {
        let mut value = json!({});
        ClaudeCodeHook::insert_hooks(&mut value).unwrap();
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
        ClaudeCodeHook::insert_hooks(&mut value).unwrap();

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
        ClaudeCodeHook::insert_hooks(&mut value).unwrap();
        ClaudeCodeHook::remove_hooks(&mut value).unwrap();

        // The non-pulse hook should remain
        let post_tool = value["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(post_tool.len(), 1);
        assert!(entry_contains_command(&post_tool[0], "other-tool do something"));
    }

    #[test]
    fn test_installed_hook_counts_partial() {
        // Simulate an old install with only 3 hooks
        let mut value = json!({});
        ClaudeCodeHook::insert_hooks(&mut value).unwrap();

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
}
