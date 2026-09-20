use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use serde::Serialize;

use crate::error::AppError;
use crate::protocol::OutputDetail;

const EVALUATE_AFTER_HELP: &str = "Examples:\n  jevctl evaluate request.json\n  cat request.json | jevctl evaluate\n  jevctl evaluate request.json --full --pretty";
const VALIDATE_AFTER_HELP: &str =
    "Examples:\n  jevctl validate request.json\n  cat request.json | jevctl validate";
const DOCTOR_AFTER_HELP: &str = "Example:\n  jevctl doctor --json";

#[derive(Debug, Parser)]
#[command(
    name = "jevctl",
    version,
    about = "Fast semantic evaluation for agent workflows"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Evaluate a batch of semantic questions.
    #[command(after_help = EVALUATE_AFTER_HELP)]
    Evaluate(EvaluateArgs),
    /// Validate an evaluation request without network access.
    #[command(after_help = VALIDATE_AFTER_HELP)]
    Validate(InputArgs),
    /// Check local readiness without making an API request.
    #[command(after_help = DOCTOR_AFTER_HELP)]
    Doctor(DoctorArgs),
}

#[derive(Debug, Args)]
pub struct EvaluateArgs {
    /// JSON request file, or - for stdin.
    #[arg(value_name = "PATH", default_value = "-")]
    pub input: PathBuf,
    /// Include normalized probability distributions.
    #[arg(long)]
    pub full: bool,
    /// Format JSON for human inspection.
    #[arg(long)]
    pub pretty: bool,
}

impl EvaluateArgs {
    /// Select the normalized output detail requested by the caller.
    pub fn detail(&self) -> OutputDetail {
        if self.full {
            OutputDetail::Full
        } else {
            OutputDetail::Compact
        }
    }
}

#[derive(Debug, Args)]
pub struct InputArgs {
    /// JSON request file, or - for stdin.
    #[arg(value_name = "PATH", default_value = "-")]
    pub input: PathBuf,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Emit compact JSON instead of text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Serialize)]
struct DoctorReport {
    api_key: &'static str,
    ready: bool,
}

/// Report whether the user-provided environment is ready for online evaluation.
pub fn run_doctor(args: DoctorArgs) -> Result<(), AppError> {
    let ready = std::env::var_os("TYPESAFE_API_KEY").is_some_and(|value| !value.is_empty());
    if args.json {
        let report = DoctorReport {
            api_key: if ready { "available" } else { "missing" },
            ready,
        };
        println!("{}", serde_json::to_string(&report)?);
    } else {
        println!("api_key: {}", if ready { "available" } else { "missing" });
    }
    Ok(())
}

/// Write one JSON result to stdout with no surrounding status text.
pub fn print_json(value: &impl Serialize, pretty: bool) -> Result<(), AppError> {
    let output = if pretty {
        serde_json::to_string_pretty(value)?
    } else {
        serde_json::to_string(value)?
    };
    println!("{output}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Command};

    #[test]
    fn validate_defaults_to_stdin() {
        let cli = Cli::try_parse_from(["jevctl", "validate"]).unwrap();
        let Command::Validate(args) = cli.command else {
            panic!("expected validate command");
        };
        assert_eq!(args.input.to_string_lossy(), "-");
    }

    #[test]
    fn evaluate_defaults_to_compact_stdin() {
        let cli = Cli::try_parse_from(["jevctl", "evaluate"]).unwrap();
        let Command::Evaluate(args) = cli.command else {
            panic!("expected evaluate command");
        };
        assert_eq!(args.input.to_string_lossy(), "-");
        assert_eq!(args.detail(), crate::protocol::OutputDetail::Compact);
        assert!(!args.pretty);
    }

    #[test]
    fn doctor_json_parses() {
        let cli = Cli::try_parse_from(["jevctl", "doctor", "--json"]).unwrap();
        let Command::Doctor(args) = cli.command else {
            panic!("expected doctor command");
        };
        assert!(args.json);
    }
}
