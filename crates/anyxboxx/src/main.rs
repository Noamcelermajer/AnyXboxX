//! `anyxboxx`: AnyXboxX's command line in Rust (docs/architecture-rust.md).
//!
//! `anyxboxx run --game-dir <folder> [...]` replaces `tools/lancer_victory.ps1`.

mod run;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "anyxboxx",
    version,
    about = "AnyXboxX: run Xbox Series executables on Windows"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build VIctory and launch it on your own game folder (replaces tools/lancer_victory.ps1).
    Run(run::RunArgs),
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Run(args) => {
            let Some(repo) = run::find_repo(args.repo.as_deref()) else {
                eprintln!("anyxboxx run: repository not found (pass --repo or set ANYXBOXX_REPO).");
                return ExitCode::FAILURE;
            };
            match run::run(&args, &repo) {
                // VIctory's exit code passed on whole (a crash code such as 0xC0000005 included).
                Ok(code) => std::process::exit(code),
                Err(error) => {
                    eprintln!("anyxboxx run: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
