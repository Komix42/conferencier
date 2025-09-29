//! Basic example showcasing loading, reading, updating, and saving with conferencier.
//!
//! Run with:
//! ```shell
//! cargo run --example basic_usage
//! ```

use conferencier::{Confer, ConferModule, Result};
use conferencier::confer_module::ConferModule as _;

/// Minimal application settings stored in the `[App]` section of a TOML document.
#[derive(ConferModule)]
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
    // Seed the configuration store from a TOML string. Files work the same via `Confer::from_file`.
    let store = Confer::from_string(
        r#"[App]
port = 8080
banner = "Welcome!"
"#,
    )?;

    // Load a typed module. The derive macro handles fetching each field using the correct getter.
    let module = AppSettings::from_confer(store.clone()).await?;

    {
        let guard = module.read().await;
        println!("Initial host: {}", guard.host);
        println!("Initial port: {}", guard.port);
        println!("Banner: {:?}", guard.banner);
    }

    {
        // Update the configuration through the typed module.
        let mut guard = module.write().await;
        guard.host = "127.0.0.1".into();
        guard.banner = None; // Removing the banner will delete the key during save.
    }

    // Persist the changes back into the TOML store.
    AppSettings::save(&module, store.clone()).await?;

    // Fetch a raw TOML string to show the result.
    let serialized = store.save_str().await?;
    println!("Updated TOML:\n{}", serialized);

    Ok(())
}
