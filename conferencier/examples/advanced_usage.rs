//! Advanced example demonstrating multiple modules sharing the same `Confer` store,
//! attribute customisations, and coordinated persistence back to disk.
//!
//! Run with:
//! ```shell
//! cargo run --example advanced_usage
//! ```

use std::{fs, path::PathBuf};

use conferencier::{Confer, ConferModule, Result};
use conferencier::confer_module::ConferModule as _;
use tokio::try_join;

/// Server runtime configuration.
#[derive(ConferModule)]
#[confer(section = "Server")]
struct ServerProfile {
    /// Rename demonstrates mapping struct fields to different TOML keys.
    #[confer(rename = "addr")]
    #[confer(default = "0.0.0.0")]
    bind_address: String,

    /// Required value – if the key is missing `from_confer` will return an error.
    port: u16,

    /// Default values for vectors come from array literals.
    #[confer(default = ["api", "web"])]
    roles: Vec<String>,

    /// Optional vector: absence of the key yields `None` and saving removes it again.
    #[confer(rename = "allow_ips")]
    allowed_ips: Option<Vec<String>>,

    /// Runtime cache that is never serialised back into TOML.
    #[confer(ignore, init = "Vec::new()")]
    connection_log: Vec<String>,
}

/// Feature toggles that live in a different TOML section.
#[derive(ConferModule)]
#[confer(section = "Features")]
struct FeatureToggles {
    /// Simple boolean default.
    #[confer(default = true)]
    realtime_metrics: bool,

    /// Optional list of beta features – omitted when empty.
    #[confer(rename = "beta")]
    beta_features: Option<Vec<String>>,

    /// Example of nested defaults and renames.
    #[confer(rename = "rollout")]
    #[confer(default = "2025-01-01T00:00:00Z")]
    rollout_start: toml::value::Datetime,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = example_config_path();
    // Load configuration from disk so the example mirrors real-world usage.
    let store = Confer::from_file(&config_path)?;
    println!("Loaded configuration from {}", config_path.display());

    // Load both modules concurrently – they share the same `Confer` instance.
    let (server, toggles) = try_join!(
        ServerProfile::from_confer(store.clone()),
        FeatureToggles::from_confer(store.clone())
    )?;

    print_configuration(&server, &toggles).await;

    // Simulate runtime updates driven by business logic.
    update_server_roles(&server).await;
    retire_beta_features(&toggles).await;

    // Persist both modules back into the shared store.
    try_join!(
        ServerProfile::save(&server, store.clone()),
        FeatureToggles::save(&toggles, store.clone())
    )?;

    // Export the resulting TOML to stdout and to an output file inside `target/`.
    let snapshot = store.save_str().await?;
    println!("\nFinal snapshot:\n{}", snapshot);

    let snapshot_path = example_snapshot_path();
    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent).map_err(conferencier::ConferError::from)?;
    }
    store.save_file(&snapshot_path).await?;
    println!(
        "Snapshot also written to {}",
        snapshot_path.display()
    );

    Ok(())
}

/// Helper that prints the current state with some lightweight formatting.
async fn print_configuration(server: &conferencier::confer_module::SharedConferModule<ServerProfile>, toggles: &conferencier::confer_module::SharedConferModule<FeatureToggles>) {
    use tokio::join;

    let (server_guard, toggles_guard) = join!(server.read(), toggles.read());
    let server = server_guard;
    let toggles = toggles_guard;

    println!("Server listening on {}:{}", server.bind_address, server.port);
    println!("Roles: {:?}", server.roles);
    println!("Allowed IPs: {:?}", server.allowed_ips);
    println!("Realtime metrics enabled: {}", toggles.realtime_metrics);
    println!("Beta features: {:?}", toggles.beta_features);
    println!("Rollout starting at: {}", toggles.rollout_start);
}

/// Add a new deployment role and track an audit message in the ignored cache.
async fn update_server_roles(server: &conferencier::confer_module::SharedConferModule<ServerProfile>) {
    let mut guard = server.write().await;
    guard.roles.push("admin".into());
    guard.connection_log.push("Roles updated to include 'admin'".into());
    guard.allowed_ips = Some(vec!["10.0.0.0/24".into(), "192.168.1.0/24".into()]);
}

/// Drop beta features once they have been fully rolled out.
async fn retire_beta_features(toggles: &conferencier::confer_module::SharedConferModule<FeatureToggles>) {
    let mut guard = toggles.write().await;
    guard.beta_features = None;
}

fn example_config_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/data/advanced_config.toml")
}

fn example_snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/advanced_usage_snapshot.toml")
}
