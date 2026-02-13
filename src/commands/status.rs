use crate::{
    commands::registered_hooks,
    config::ConfigStore,
    error::{PulseError, Result},
    hooks::HookStatus,
    http::TraceHttpClient,
};

pub async fn run_status() -> Result<()> {
    let config = match ConfigStore::load() {
        Ok(cfg) => cfg,
        Err(PulseError::ConfigMissing) => {
            println!("Pulse is not initialized. Run `pulse init` first.");
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    println!("Configuration");
    println!("  API URL     : {}", config.api_url);
    println!("  Project ID  : {}", config.project_id);
    let config_path = ConfigStore::config_path()?;
    println!("  Config file : {}", config_path.display());
    println!("  API key     : {}", mask_key(&config.api_key));

    println!("\nConnectivity");
    match TraceHttpClient::new(&config) {
        Ok(client) => match client.health_check().await {
            Ok(_) => println!("  Trace service reachable"),
            Err(err) => println!("  Unable to reach trace service: {err}"),
        },
        Err(err) => println!("  Invalid configuration: {err}"),
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
}
