pub mod connect;
pub mod dashboard;
pub mod disconnect;
pub mod emit;
pub mod init;
pub mod setup;
pub mod status;

use crate::error::Result;
use crate::hooks::{ClaudeCodeHook, OpenClawHook, OpenCodeHook, ToolHook};

pub use connect::run_connect;
pub use dashboard::{DashboardArgs, run_dashboard};
pub use disconnect::run_disconnect;
pub use emit::{EmitArgs, run_emit};
pub use init::{InitArgs, run_init};
pub use setup::{SetupArgs, run_setup};
pub use status::run_status;

pub(crate) fn registered_hooks() -> Result<Vec<Box<dyn ToolHook>>> {
    let mut hooks: Vec<Box<dyn ToolHook>> = Vec::new();
    hooks.push(Box::new(ClaudeCodeHook::new()?));
    hooks.push(Box::new(OpenCodeHook::new()?));
    hooks.push(Box::new(OpenClawHook::new()?));
    Ok(hooks)
}
