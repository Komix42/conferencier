//! Basic example showcasing loading, reading, updating, and saving with conferencier.
//!
//! Run with:
//! ```shell
//! cargo run --example basic_usage
//! ```

use conferencier::{ConfigScope, ConfigStore, Result};

/// Minimal application settings stored in the `[App]` section of a TOML document.
#[derive(ConfigScope)]
#[confer(section = "App")]
struct AppSettings {
    /// Mandatory value fetched from TOML (errors if missing).
    port: u16,
    /// Provide a default when the key is absent.
    #[confer(default = "localhost")]
    host: String,
    /// Optional field becomes `None` when the key is missing.
    banner: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Seed the configuration store from a TOML string. Files work the same via `ConfigStore::from_file`.
    let store = ConfigStore::from_str(
        r#"[App]
port = 8080
banner = "Welcome!"
"#,
    )?;

    // Load a typed scope. The derive macro handles fetching each field using the correct getter.
    let scope = AppSettings::from_store(&store)?;

    {
        let guard = scope.read().await;
        println!("Initial host: {}", guard.host);
        println!("Initial port: {}", guard.port);
        println!("Banner: {:?}", guard.banner);
    }

    {
        // Update the configuration through the typed scope.
        let mut guard = scope.write().await;
        guard.host = "127.0.0.1".into();
        guard.banner = None; // Removing the banner will delete the key during save.
    }

    // Persist the changes back into the TOML store.
    AppSettings::save(&scope, &store).await?;

    // Fetch a raw TOML string to show the result.
    let serialized = store.save_str()?;
    println!("Updated TOML:\n{}", serialized);

    Ok(())
}
