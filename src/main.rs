use clap::{Parser, Subcommand};
use std::process::ExitCode;

use pulse::commands::{
    ConnectArgs, DashboardArgs, EmitArgs, InitArgs, LogsArgs, SetupArgs, UpArgs, run_connect,
    run_dashboard, run_disconnect, run_down, run_emit, run_init, run_logs, run_restart, run_setup,
    run_status, run_up,
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
    Connect(ConnectArgs),
    Disconnect,
    Status,
    Up(UpArgs),
    Down,
    Restart(UpArgs),
    Logs(LogsArgs),
    Emit(EmitArgs),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result: Result<()> = match cli.command {
        Commands::Init(args) => run_init(args).await,
        Commands::Setup(args) => run_setup(args).await,
        Commands::Dashboard(args) => run_dashboard(args).await,
        Commands::Connect(args) => run_connect(args).await,
        Commands::Disconnect => run_disconnect(),
        Commands::Status => run_status().await,
        Commands::Up(args) => run_up(args).await,
        Commands::Down => run_down().await,
        Commands::Restart(args) => run_restart(args).await,
        Commands::Logs(args) => run_logs(args).await,
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
