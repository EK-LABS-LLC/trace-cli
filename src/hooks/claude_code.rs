use std::{fs, io::ErrorKind, path::PathBuf};

use dirs::home_dir;
use serde_json::{Map, Value, json};

use crate::error::{PulseError, Result};

use super::{HookStatus, ToolHook};

const CLAUDE_SETTINGS: &str = ".claude/settings.json";
const CLAUDE_TOOL_NAME: &str = "Claude Code";
pub const CLAUDE_SOURCE: &str = "claude_code";
const HOOK_DEFINITIONS: &[(&str, &str)] = &[
    ("PostToolUse", "pulse emit post_tool_use"),
    ("Stop", "pulse emit stop"),
    ("SessionStart", "pulse emit session_start"),
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

    pub fn source(&self) -> &'static str {
        CLAUDE_SOURCE
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
        let connected = has_all_hooks(&value);
        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected,
            modified: false,
            path: Some(self.settings_path.clone()),
            message: None,
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
        let connected = has_all_hooks(&value);
        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected,
            modified: changed,
            path: Some(self.settings_path.clone()),
            message: None,
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
        Ok(HookStatus {
            tool: self.tool_name(),
            detected: true,
            connected: has_all_hooks(&value),
            modified: changed,
            path: Some(self.settings_path.clone()),
            message: None,
        })
    }
}

fn has_all_hooks(value: &Value) -> bool {
    let hooks_map = match value
        .as_object()
        .and_then(|obj| obj.get("hooks"))
        .and_then(|hooks| hooks.as_object())
    {
        Some(map) => map,
        None => return false,
    };

    HOOK_DEFINITIONS.iter().all(|(event, command)| {
        hooks_map
            .get(*event)
            .and_then(|value| value.as_array())
            .map(|array| {
                array
                    .iter()
                    .any(|entry| entry_contains_command(entry, command))
            })
            .unwrap_or(false)
    })
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
