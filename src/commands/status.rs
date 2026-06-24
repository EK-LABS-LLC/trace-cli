use crate::{
    commands::local::local_server_status,
    commands::registered_hooks,
    config::{ConfigMode, ConfigStore},
    error::{PulseError, Result},
    hooks::HookStatus,
    http::TraceHttpClient,
};

pub async fn run_status() -> Result<()> {
    let config = match ConfigStore::load() {
        Ok(cfg) => cfg,
        Err(PulseError::ConfigMissing) => {
            println!(
                "Pulse is not initialized. Run `pulse connect` or `pulse setup --local` first."
            );
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    println!("Configuration");
    println!(
        "  Mode        : {}",
        match config.effective_mode() {
            ConfigMode::Local => "local",
            ConfigMode::Remote => "remote",
        }
    );
    println!("  API URL     : {}", config.api_url);
    println!("  Project ID  : {}", config.project_id);
    let config_path = ConfigStore::config_path()?;
    println!("  Config file : {}", config_path.display());
    println!("  API key     : {}", mask_key(&config.api_key));

    match config.effective_mode() {
        ConfigMode::Local => {
            let status = local_server_status(&config).await?;
            println!("\nLocal server");
            println!(
                "  Running     : {}",
                if status.running { "yes" } else { "no" }
            );
            println!(
                "  Healthy     : {}",
                if status.healthy { "yes" } else { "no" }
            );
            println!(
                "  PID         : {}",
                status
                    .pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "(none)".to_string())
            );
            println!("  PID file    : {}", status.pid_path.display());
            println!("  Logs        : {}", status.log_path.display());
            println!("  Dashboard   : {}", config.api_url);
        }
        ConfigMode::Remote => {
            println!("\nConnectivity");
            match TraceHttpClient::new(&config) {
                Ok(client) => match client.health_check().await {
                    Ok(_) => println!("  Trace service reachable"),
                    Err(err) => println!("  Unable to reach trace service: {err}"),
                },
                Err(err) => println!("  Invalid configuration: {err}"),
            }
            println!("  Dashboard   : {}", config.api_url);
        }
    }

    println!("\nHooks");
    for hook in registered_hooks()? {
        let status = hook.status()?;
        print_hook_status(&status);
    }

    Ok(())
}

fn mask_key(key: &str) -> String {
    if key.is_empty() {
        return "(empty)".to_string();
    }
    let preview: String = key.chars().take(4).collect();
    format!("{}***", preview)
}

fn print_hook_status(status: &HookStatus) {
    if !status.detected {
        println!(
            "  - {}: {}",
            status.tool,
            status
                .message
                .as_deref()
                .unwrap_or("Tool not detected on this machine")
        );
        return;
    }

    let suffix = status
        .path
        .as_ref()
        .map(|path| format!(" ({})", path.display()))
        .unwrap_or_default();

    if status.connected {
        println!("  - {}: connected{}", status.tool, suffix);
    } else {
        println!("  - {}: disconnected{}", status.tool, suffix);
    }

    if status.total_hooks > 0 {
        println!(
            "    {}/{} hooks installed",
            status.installed_hooks, status.total_hooks
        );
        if !status.installed_hook_names.is_empty() {
            println!("    {}", status.installed_hook_names.join(", "));
        }
        if !status.connected && status.installed_hooks < status.total_hooks {
            println!("    Run `pulse install-hooks` to install missing hooks");
        }
    }
}
