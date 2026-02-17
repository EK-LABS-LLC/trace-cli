use crate::{commands::registered_hooks, config::ConfigStore, error::Result, hooks::HookStatus};

pub fn run_connect() -> Result<()> {
    // Ensure configuration exists before wiring hooks.
    ConfigStore::load()?;

    println!("Detecting supported tools...");
    let hooks = registered_hooks()?;
    let mut any_connected = false;

    for hook in hooks {
        let status = hook.connect()?;
        print_connect_summary(&status);
        if status.detected && status.connected {
            any_connected = true;
        }
    }

    if any_connected {
        Ok(())
    } else {
        println!(
            "No supported tools detected. Launch Claude Code at least once so we can locate its settings."
        );
        Ok(())
    }
}

fn print_connect_summary(status: &HookStatus) {
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
        if status.modified {
            println!(
                "- {}: hooks installed{}",
                status.tool,
                format_path_suffix(status)
            );
        } else {
            println!(
                "- {}: already connected{}",
                status.tool,
                format_path_suffix(status)
            );
        }
    } else {
        println!(
            "- {}: unable to inject hooks{}",
            status.tool,
            format_path_suffix(status)
        );
    }

    print_hook_details(status);
}

fn print_hook_details(status: &HookStatus) {
    if status.total_hooks == 0 {
        return;
    }
    println!(
        "    {}/{} hooks installed",
        status.installed_hooks, status.total_hooks
    );
    if !status.installed_hook_names.is_empty() {
        println!("    {}", status.installed_hook_names.join(", "));
    }
    if status.installed_hooks < status.total_hooks {
        println!("    Run `pulse connect` to install missing hooks");
    }
}

fn format_path_suffix(status: &HookStatus) -> String {
    status
        .path
        .as_ref()
        .map(|path| format!(" ({})", path.display()))
        .unwrap_or_default()
}
