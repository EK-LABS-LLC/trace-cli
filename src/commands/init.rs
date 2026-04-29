use clap::Args;

use crate::{
    commands::connect::{ConnectArgs, run_connect},
    error::Result,
};

#[derive(Debug, Args)]
pub struct InitArgs {
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
}

pub async fn run_init(args: InitArgs) -> Result<()> {
    println!("`pulse init` is deprecated. Use `pulse connect` instead.");
    run_connect(ConnectArgs {
        api_url: args.api_url,
        api_key: args.api_key,
        project_id: args.project_id,
        no_validate: args.no_validate,
        no_hooks: true,
    })
    .await
}
