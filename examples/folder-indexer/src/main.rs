//! Folder indexer: an example application configured with conferencier.
//!
//! The application creates and changes its own configuration file. See
//! `README.md` for usage and architecture.

mod config;

use std::path::PathBuf;
use std::process::ExitCode;

use crate::config::{BoxError, Settings};

type AppResult<T> = Result<T, BoxError>;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> AppResult<()> {
    let path = config_path(std::env::args().skip(1))?;
    let (settings, created) = Settings::open(path).await?;
    if created {
        println!(
            "Created {} with the default configuration",
            settings.path.display()
        );
    } else {
        println!("Loaded configuration from {}", settings.path.display());
    }
    for section in settings.unexpected_sections() {
        println!("warning: unexpected section [{section}] is ignored; is its name misspelled?");
    }
    print!("{}", settings.effective_toml().await?);
    Ok(())
}

/// The configuration file: `--config <path>`, or `indexer.toml` in the package
/// folder, independent of the working directory.
fn config_path(mut args: impl Iterator<Item = String>) -> AppResult<PathBuf> {
    match (args.next().as_deref(), args.next(), args.next()) {
        (None, _, _) => Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("indexer.toml")),
        (Some("--config"), Some(path), None) => Ok(PathBuf::from(path)),
        _ => Err("usage: folder-indexer [--config <path>]".into()),
    }
}
