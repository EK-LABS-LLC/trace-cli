use clap::{Parser, Subcommand};
use std::process::ExitCode;

use pulse::commands::{
    ConnectArgs, DashboardArgs, EmitArgs, EmitCodexArgs, InitArgs, InstallHooksArgs, LogsArgs,
    SetupArgs, UpArgs, UpdateArgs, maybe_prompt_update, run_connect, run_dashboard, run_disconnect,
    run_down, run_emit, run_emit_codex, run_init, run_install_hooks, run_logs, run_restart,
    run_setup, run_status, run_up, run_update,
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
    #[command(hide = true)]
    EmitCodex(EmitCodexArgs),
    InstallHooks(InstallHooksArgs),
    Update(UpdateArgs),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    if should_check_for_updates(&cli.command) {
        maybe_prompt_update().await;
    }

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
        Commands::EmitCodex(args) => {
            run_emit_codex(args).await;
            Ok(())
        }
        Commands::InstallHooks(args) => run_install_hooks(args).await,
        Commands::Update(args) => run_update(args).await,
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn should_check_for_updates(command: &Commands) -> bool {
    matches!(
        command,
        Commands::Dashboard(_) | Commands::Up(_) | Commands::Restart(_)
    )
}

#[cfg(test)]
mod tests {
    use super::{Commands, should_check_for_updates};
    use pulse::commands::{DashboardArgs, LogsArgs, UpArgs};

    #[test]
    fn update_prompt_runs_for_startup_commands() {
        assert!(should_check_for_updates(&Commands::Dashboard(
            DashboardArgs {
                api_url: None,
                dashboard_url: None,
                no_open: false,
            }
        )));
        assert!(should_check_for_updates(&Commands::Up(UpArgs {
            open: false
        })));
        assert!(should_check_for_updates(&Commands::Restart(UpArgs {
            open: false
        })));
    }

    #[test]
    fn update_prompt_skips_non_startup_commands() {
        assert!(!should_check_for_updates(&Commands::Status));
        assert!(!should_check_for_updates(&Commands::Logs(LogsArgs {
            follow: false,
            lines: 10,
        })));
    }
}
