use std::io::{self, Write};

use crate::{
    config::{ConfigStore, PulseConfig},
    error::{PulseError, Result},
    http::TraceHttpClient,
};

pub async fn run_init() -> Result<()> {
    println!("Pulse CLI setup");
    println!("----------------");

    let api_url = prompt_required("Trace service URL (e.g. https://pulse.example.com)", false)?;
    let api_key = prompt_required("API key", true)?;
    let project_id = prompt_required("Project ID", false)?;

    let config = PulseConfig {
        api_url,
        api_key,
        project_id,
    }
    .sanitized();

    println!("\nValidating credentials...");
    let client = TraceHttpClient::new(&config)?;
    client.health_check().await.map_err(|err| {
        PulseError::message(format!(
            "Failed to contact trace service at {}: {err}",
            config.api_url
        ))
    })?;

    ConfigStore::save(&config)?;
    let path = ConfigStore::config_path()?;
    println!("Configuration saved to {}", path.display());
    Ok(())
}

fn prompt_required(prompt: &str, secret: bool) -> Result<String> {
    loop {
        let value = if secret {
            rpassword::prompt_password(format!("{}: ", prompt))?
        } else {
            print!("{}: ", prompt);
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
