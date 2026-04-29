use std::io::{self, Write};

use clap::Args;

use crate::{
    commands::registered_hooks,
    config::{ConfigMode, ConfigStore, PulseConfig},
    error::{PulseError, Result},
    hooks::HookStatus,
    http::TraceHttpClient,
};

#[derive(Debug, Args)]
pub struct ConnectArgs {
    /// Trace service URL (e.g. https://pulse.example.com)
    #[arg(long)]
    pub api_url: Option<String>,
    /// API key for authentication
    #[arg(long)]
    pub api_key: Option<String>,
    /// Project ID
    #[arg(long)]
    pub project_id: Option<String>,
    /// Skip health check validation
    #[arg(long)]
    pub no_validate: bool,
    /// Skip automatic hook installation
    #[arg(long)]
    pub no_hooks: bool,
}

pub async fn run_connect(args: ConnectArgs) -> Result<()> {
    let api_url = match args.api_url {
        Some(v) => v,
        None => prompt_required("Trace service URL (e.g. https://pulse.example.com)", false)?,
    };

    let api_key = match args.api_key {
        Some(v) => v,
        None => prompt_required("API key", true)?,
    };

    let project_id = match args.project_id {
        Some(v) => v,
        None => prompt_required("Project ID", false)?,
    };

    let config = PulseConfig {
        mode: Some(ConfigMode::Remote),
        api_url,
        api_key,
        project_id,
        server_command: None,
        local_email: None,
        local_password: None,
    }
    .sanitized();

    if !args.no_validate {
        println!("Validating connectivity...");
        let client = TraceHttpClient::new(&config)?;
        client.health_check().await.map_err(|err| {
            PulseError::message(format!(
                "Failed to contact trace service at {}: {err}",
                config.api_url
            ))
        })?;
    }

    ConfigStore::save(&config)?;
    let path = ConfigStore::config_path()?;
    println!("Configuration saved to {}", path.display());

    if args.no_hooks {
        println!("Skipped agent integration setup (--no-hooks).");
        return Ok(());
    }

    install_hooks()
}

pub fn install_hooks() -> Result<()> {
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

fn prompt_required(prompt: &str, secret: bool) -> Result<String> {
    loop {
        let value = if secret {
            rpassword::prompt_password(format!("{prompt}: "))?
        } else {
            print!("{prompt}: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        };

        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }

        println!("Value required");
    }
}
