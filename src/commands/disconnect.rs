use crate::{commands::registered_hooks, config::ConfigStore, error::Result, hooks::HookStatus};

pub fn run_disconnect() -> Result<()> {
    ConfigStore::load()?;

    println!("Removing hooks...");
    let hooks = registered_hooks()?;
    for hook in hooks {
        let status = hook.disconnect()?;
        print_disconnect_summary(&status);
    }

    Ok(())
}

fn print_disconnect_summary(status: &HookStatus) {
    if !status.detected {
        println!(
            "- {}: {}",
            status.tool,
            status
                .message
                .as_deref()
                .unwrap_or("Tool not detected on this machine")
        );
        return;
    }

    if status.connected {
        println!(
            "- {}: hooks still present{}",
            status.tool,
            format_path_suffix(status)
        );
    } else if status.modified {
        println!(
            "- {}: hooks removed{}",
            status.tool,
            format_path_suffix(status)
        );
    } else {
        println!(
            "- {}: no hooks to remove{}",
            status.tool,
            format_path_suffix(status)
        );
    }
}

fn format_path_suffix(status: &HookStatus) -> String {
    status
        .path
        .as_ref()
        .map(|path| format!(" ({})", path.display()))
        .unwrap_or_default()
}
