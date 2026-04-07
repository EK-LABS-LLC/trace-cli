use std::{fs, path::PathBuf};

use clap::{Args, Subcommand};

use crate::{
    config::ConfigStore,
    error::{PulseError, Result},
    http::{SftExportQuery, TraceHttpClient},
    server,
};

#[derive(Debug, Subcommand)]
pub enum ExportCommands {
    Sft(ExportSftArgs),
}

#[derive(Debug, Args)]
pub struct ExportSftArgs {
    /// Export mode: plain chat messages or tool-aware messages
    #[arg(long, default_value = "plain", value_parser = ["plain", "tool"])]
    pub mode: String,
    /// Output format
    #[arg(long, default_value = "jsonl", value_parser = ["json", "jsonl"])]
    pub format: String,
    /// Filter to a single session
    #[arg(long)]
    pub session_id: Option<String>,
    /// Filter by span source
    #[arg(long, value_parser = ["claude_code", "opencode", "openclaw"])]
    pub source: Option<String>,
    /// Start of time range (ISO timestamp, YYYY-MM-DD, or epoch)
    #[arg(long)]
    pub date_from: Option<String>,
    /// End of time range (ISO timestamp, YYYY-MM-DD, or epoch)
    #[arg(long)]
    pub date_to: Option<String>,
    /// Maximum number of exported sessions
    #[arg(long, default_value_t = 100)]
    pub max_sessions: u32,
    /// Maximum number of spans to scan
    #[arg(long, default_value_t = 5000)]
    pub max_spans: u32,
    /// Maximum number of traces to scan
    #[arg(long, default_value_t = 5000)]
    pub max_traces: u32,
    /// Write export output to a file instead of stdout
    #[arg(long)]
    pub output: Option<PathBuf>,
}

pub async fn run_export(command: ExportCommands) -> Result<()> {
    match command {
        ExportCommands::Sft(args) => run_export_sft(args).await,
    }
}

async fn run_export_sft(args: ExportSftArgs) -> Result<()> {
    let config = ConfigStore::load()?;
    server::ensure_server(&config).await?;
    let client = TraceHttpClient::new(&config)?;
    let query = SftExportQuery {
        session_id: args.session_id,
        source: args.source,
        date_from: args.date_from,
        date_to: args.date_to,
        max_sessions: args.max_sessions,
        max_spans: args.max_spans,
        max_traces: args.max_traces,
        mode: args.mode,
        format: args.format.clone(),
    };

    let output = if args.format == "json" {
        let response = client.export_sft_json(&query).await?;
        serde_json::to_string_pretty(&response)?
    } else {
        client.export_sft_jsonl(&query).await?
    };

    if let Some(path) = args.output {
        fs::write(&path, output).map_err(|err| {
            PulseError::message(format!("failed to write {}: {err}", path.display()))
        })?;
        println!("Wrote SFT export to {}", path.display());
    } else {
        print!("{output}");
    }

    Ok(())
}
