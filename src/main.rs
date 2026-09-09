use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

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
        #[arg(long)]
        require_cow: bool,
        #[arg(long)]
        json: bool,
        #[arg(short, long)]
        verbose: bool,
    },
    /// Inspect Copy-on-Write support for a path
    Info {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum StrategyArg {
    #[default]
    Auto,
    Cow,
    Copy,
}

fn main() -> Result<()> {
    let _cli = Cli::parse();
    bail!("command implementation is not connected")
}
