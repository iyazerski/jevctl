pub mod cli;
pub mod error;
pub mod input;
pub mod protocol;
pub mod validation;

use clap::Parser;

use crate::cli::{Cli, Command};
use crate::error::AppError;

/// Parse arguments, run the requested command, and return its process exit code.
pub fn run() -> i32 {
    match run_inner() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("jevctl: {error}");
            error.exit_code()
        }
    }
}

fn run_inner() -> Result<(), AppError> {
    match Cli::parse().command {
        Command::Validate(args) => {
            let request = input::read_request(&args.input)?;
            validation::validate_request(&request)
        }
        Command::Doctor(args) => cli::run_doctor(args),
    }
}
