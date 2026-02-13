mod claude_code;

pub use claude_code::{CLAUDE_SOURCE, ClaudeCodeHook};

use crate::error::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct HookStatus {
    pub tool: &'static str,
    pub detected: bool,
    pub connected: bool,
    pub modified: bool,
    pub path: Option<PathBuf>,
    pub message: Option<String>,
}

impl HookStatus {
    pub fn not_detected(tool: &'static str, path: PathBuf) -> Self {
        Self {
            tool,
            detected: false,
            connected: false,
            modified: false,
            path: Some(path.clone()),
            message: Some(format!(
                "Tool not detected. Expected settings at {}",
                path.display()
            )),
        }
    }
}

pub trait ToolHook {
    fn tool_name(&self) -> &'static str;
    fn status(&self) -> Result<HookStatus>;
    fn connect(&self) -> Result<HookStatus>;
    fn disconnect(&self) -> Result<HookStatus>;
}
