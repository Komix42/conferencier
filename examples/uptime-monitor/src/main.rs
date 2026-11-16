//! Uptime monitor: an example application configured with conferencier.

mod config;

use std::path::PathBuf;
use std::process::ExitCode;

use conferencier::{ConfigScope, ConfigStore};

use crate::config::{HttpConfig, MonitorConfig};

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

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = default_config_path();
    let store = ConfigStore::from_file(&path).await?;
    let http = HttpConfig::from_store(&store)?;
    let monitor = MonitorConfig::from_store(&store)?;
    config::validate(&*monitor.read().await)?;

    println!("Loaded configuration from {}", path.display());
    let http = http.read().await;
    println!("HTTP server: http://{}:{}/", http.address, http.port);
    let monitor = monitor.read().await;
    println!(
        "Monitor: enabled={}, interval={:?}, timeout={:?}",
        monitor.enabled,
        monitor.interval(),
        monitor.timeout()
    );
    for target in &monitor.targets {
        println!("  target {} at {}", target.name, target.address);
    }
    Ok(())
}

/// The hand-written default configuration, independent of the working directory.
fn default_config_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("config")
        .join("monitor.toml")
}
