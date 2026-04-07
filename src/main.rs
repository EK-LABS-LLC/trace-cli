use clap::{Parser, Subcommand};
use std::process::ExitCode;

use pulse::commands::{
    DashboardArgs, EmitArgs, ExportCommands, InitArgs, SetupArgs, run_connect, run_dashboard,
    run_disconnect, run_emit, run_export, run_init, run_setup, run_status,
};
use pulse::error::Result;

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
    Init(InitArgs),
    Setup(SetupArgs),
    Dashboard(DashboardArgs),
    Export {
        #[command(subcommand)]
        command: ExportCommands,
    },
    Connect,
    Disconnect,
    Status,
    Emit(EmitArgs),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result: Result<()> = match cli.command {
        Commands::Init(args) => run_init(args).await,
        Commands::Setup(args) => run_setup(args).await,
        Commands::Dashboard(args) => run_dashboard(args).await,
        Commands::Export { command } => run_export(command).await,
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
