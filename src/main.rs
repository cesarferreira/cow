use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand, ValueEnum};
use cow::{
    CloneOptions, CloneResult, CloneStrategy, CowCapability, CowError, FilesystemInfo,
    StrategyPreference, clone_dir, inspect, install_interrupt_handler,
};
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(
    name = "cow",
    version,
    about = "Create cheap independent directory clones"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Clone a complete directory tree
    Clone {
        source: PathBuf,
        destination: PathBuf,
        #[arg(long, value_enum, default_value_t = StrategyArg::Auto)]
        strategy: StrategyArg,
        /// Require Copy-on-Write; equivalent to --strategy cow
        #[arg(long)]
        require_cow: bool,
        /// Emit stable machine-readable output
        #[arg(long)]
        json: bool,
        /// Show operation details on stderr
        #[arg(short, long)]
        verbose: bool,
    },
    /// Inspect Copy-on-Write support for a path
    Info {
        path: PathBuf,
        /// Emit stable machine-readable output
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
enum StrategyArg {
    #[default]
    Auto,
    Cow,
    Copy,
}

impl From<StrategyArg> for StrategyPreference {
    fn from(value: StrategyArg) -> Self {
        match value {
            StrategyArg::Auto => Self::Auto,
            StrategyArg::Cow => Self::Cow,
            StrategyArg::Copy => Self::Copy,
        }
    }
}

#[derive(Serialize)]
struct CloneOutput {
    source: String,
    destination: String,
    strategy: CloneStrategy,
    cow: bool,
    logical_bytes: u64,
    files: u64,
    duration_ms: u64,
}

impl From<&CloneResult> for CloneOutput {
    fn from(result: &CloneResult) -> Self {
        Self {
            source: result.source.display().to_string(),
            destination: result.destination.display().to_string(),
            strategy: result.strategy,
            cow: result.strategy.is_cow(),
            logical_bytes: result.logical_bytes,
            files: result.files,
            duration_ms: result.duration.as_millis().min(u128::from(u64::MAX)) as u64,
        }
    }
}

#[derive(Serialize)]
struct InfoOutput {
    path: String,
    platform: String,
    filesystem: Option<String>,
    cow_supported: CowCapability,
    preferred_strategy: CloneStrategy,
}

impl From<FilesystemInfo> for InfoOutput {
    fn from(info: FilesystemInfo) -> Self {
        Self {
            path: info.path.display().to_string(),
            platform: info.platform,
            filesystem: info.filesystem,
            cow_supported: info.cow_supported,
            preferred_strategy: info.preferred_strategy,
        }
    }
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    error: ErrorOutput<'a>,
}

#[derive(Serialize)]
struct ErrorOutput<'a> {
    code: &'static str,
    message: &'a str,
}

fn main() -> ExitCode {
    if let Err(error) = install_interrupt_handler() {
        eprintln!("Error: {error}");
        return ExitCode::FAILURE;
    }
    run(Cli::parse())
}

fn run(cli: Cli) -> ExitCode {
    match cli.command {
        Command::Clone {
            source,
            destination,
            strategy,
            require_cow,
            json,
            verbose,
        } => {
            if require_cow && strategy == StrategyArg::Copy {
                eprintln!("error: --require-cow cannot be used with --strategy copy");
                return ExitCode::from(2);
            }
            let strategy = if require_cow {
                StrategyPreference::Cow
            } else {
                strategy.into()
            };
            match clone_dir(source, destination, CloneOptions { strategy }) {
                Ok(result) => {
                    if json {
                        print_json(&CloneOutput::from(&result));
                    } else {
                        print_clone(&result, verbose);
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => print_error(&error, json),
            }
        }
        Command::Info { path, json } => match inspect(path) {
            Ok(info) => {
                if json {
                    print_json(&InfoOutput::from(info));
                } else {
                    print_info(&info);
                }
                ExitCode::SUCCESS
            }
            Err(error) => print_error(&error, json),
        },
    }
}

fn print_json(value: &impl Serialize) {
    println!(
        "{}",
        serde_json::to_string(value).expect("serializable output")
    );
}

fn print_error(error: &CowError, json: bool) -> ExitCode {
    if json {
        let message = error.to_string();
        let envelope = ErrorEnvelope {
            error: ErrorOutput {
                code: error.code(),
                message: &message,
            },
        };
        eprintln!(
            "{}",
            serde_json::to_string(&envelope).expect("serializable error")
        );
    } else {
        eprintln!("Error: {error}");
    }
    ExitCode::FAILURE
}

fn print_clone(result: &CloneResult, verbose: bool) {
    let verb = if result.strategy.is_cow() {
        "Cloned"
    } else {
        "Copied"
    };
    println!(
        "✓ {verb} {} → {} using {} ({})",
        result.source.display(),
        result.destination.display(),
        strategy_name(result.strategy),
        format_duration(result.duration.as_millis()),
    );
    if verbose {
        eprintln!("Files: {}", result.files);
        eprintln!("Logical size: {} bytes", result.logical_bytes);
    } else if result.strategy == CloneStrategy::Copy {
        println!("  CoW unavailable or bypassed; used regular copy.");
    }
}

fn print_info(info: &FilesystemInfo) {
    println!("Path: {}", info.path.display());
    println!("Platform: {}", info.platform);
    println!(
        "Filesystem: {}",
        info.filesystem.as_deref().unwrap_or("unknown")
    );
    println!("Copy-on-write: {}", capability_name(info.cow_supported));
    println!(
        "Preferred strategy: {}",
        strategy_name(info.preferred_strategy)
    );
}

fn strategy_name(strategy: CloneStrategy) -> &'static str {
    match strategy {
        CloneStrategy::ApfsClone => "APFS clone",
        CloneStrategy::Reflink => "reflink",
        CloneStrategy::Copy => "regular copy",
    }
}

fn capability_name(capability: CowCapability) -> &'static str {
    match capability {
        CowCapability::Supported => "supported",
        CowCapability::Unavailable => "unavailable",
        CowCapability::Unknown => "unknown",
    }
}

fn format_duration(milliseconds: u128) -> String {
    if milliseconds < 1_000 {
        format!("{milliseconds} ms")
    } else {
        format!("{:.1} s", milliseconds as f64 / 1_000.0)
    }
}
