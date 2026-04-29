use std::io::{self, Write};

use clap::Args;

use crate::{
    commands::install_hooks::install_hooks,
    config::{ConfigMode, ConfigStore, PulseConfig},
    error::{PulseError, Result},
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
