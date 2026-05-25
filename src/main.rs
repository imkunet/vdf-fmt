use std::time::Instant;

use clap::Parser;
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::cli::Cli;
use anyhow::Result;
use vdf_fmt::formatter;

pub(crate) mod cli;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(
            tracing_subscriber::filter::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let format_start = Instant::now();
    let write = args.write || args.paths.len() > 1;
    let options = formatter::Options {
        reflow_comments: args.reflow_comments,
        bare_literals: args.bare_literals,
    };

    for path in &args.paths {
        if !path.is_file() {
            error!("Input must be a file only: {}", path.display());
            std::process::exit(1);
        }

        let file_content = tokio::fs::read_to_string(path).await?;
        let formatted = if options == formatter::Options::default() {
            formatter::format(&file_content)?
        } else {
            formatter::format_with_options(&file_content, options)?
        };

        if write {
            tokio::fs::write(path, formatted).await?;
            info!("formatted {}", path.display());
        } else {
            print!("{formatted}");
        }
    }

    let format_end = Instant::now();
    let format_duration = Instant::duration_since(&format_end, format_start);
    info!(
        "finished in {}",
        humantime::format_duration(format_duration)
    );

    Ok(())
}
