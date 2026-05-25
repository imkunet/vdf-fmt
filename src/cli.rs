use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(version, about)]
pub struct Cli {
    /// Files to be formatted in VDF format
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Write to the input file, rather than outputting the formatted content to stdout
    #[arg(short, long)]
    pub write: bool,

    /// Reflow comments that look like disabled KeyValues pairs
    #[arg(long)]
    pub reflow_comments: bool,

    /// Leave true, false, and numeric values unquoted
    #[arg(long)]
    pub bare_literals: bool,
}
