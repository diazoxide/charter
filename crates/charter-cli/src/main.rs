use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "charter",
    version,
    about = "charter: run tons of harness sessions in parallel"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the plane root the current directory sits in.
    Root,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Root => {
            let cwd = match std::env::current_dir() {
                Ok(cwd) => cwd,
                Err(err) => {
                    eprintln!("charter: cannot read the current directory: {err}");
                    return ExitCode::FAILURE;
                }
            };
            match charter_core::plane::find_root(&cwd) {
                Ok(root) => {
                    println!("{}", root.display());
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("charter: {err}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
