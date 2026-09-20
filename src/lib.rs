pub mod cli;
pub mod client;
pub mod error;
pub mod input;
pub mod mcp;
pub mod protocol;
mod typesafe;
pub mod validation;

use clap::Parser;

use crate::cli::{Cli, Command};
use crate::error::AppError;

/// Parse arguments, run the requested command, and return its process exit code.
pub async fn run() -> i32 {
    match run_inner().await {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("jevctl: {error}");
            error.exit_code()
        }
    }
}

async fn run_inner() -> Result<(), AppError> {
    match Cli::parse().command {
        Command::Evaluate(args) => {
            let request = input::read_request(&args.input)?;
            validation::validate_request(&request)?;
            let client = client::EvaluationClient::from_env()?;
            let output = client.evaluate(&request, args.detail()).await?;
            cli::print_json(&output, args.pretty)
        }
        Command::Validate(args) => {
            let request = input::read_request(&args.input)?;
            validation::validate_request(&request)
        }
        Command::Doctor(args) => cli::run_doctor(args),
        Command::Mcp(args) => mcp::run(args).await,
    }
}
