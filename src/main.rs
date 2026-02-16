mod commands;
mod config;
mod error;
mod hooks;
mod http;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

use commands::{EmitArgs, run_connect, run_disconnect, run_emit, run_init, run_status};
use error::Result;

#[derive(Parser, Debug)]
#[command(
    name = "pulse",
    about = "Pulse CLI for agentic tool observability",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Init,
    Connect,
    Disconnect,
    Status,
    Emit(EmitArgs),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result: Result<()> = match cli.command {
        Commands::Init => run_init().await,
        Commands::Connect => run_connect(),
        Commands::Disconnect => run_disconnect(),
        Commands::Status => run_status().await,
        Commands::Emit(args) => {
            run_emit(args).await;
            Ok(())
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}
