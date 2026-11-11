//! Nested tables and arrays of tables mapped to plain structs.
//!
//! Run with:
//! ```shell
//! cargo run --example nested_tables
//! ```

use conferencier::{Confer, ConferModule, ConferTable, Result};

/// The `[App.server]` table. All fields have defaults, so the table may be omitted.
#[derive(ConferTable)]
struct Server {
    #[confer(default = "localhost")]
    host: String,
    #[confer(default = 8080)]
    port: u16,
}

/// One `[[App.upstream]]` entry.
#[derive(ConferTable)]
struct Upstream {
    url: String,
    #[confer(default = 1)]
    weight: u32,
}

#[derive(ConferModule)]
#[confer(section = "App")]
struct AppSettings {
    name: String,
    /// A sub-table; written as `[App.server]` or `server = { ... }`.
    #[confer(table)]
    server: Server,
    /// An array of tables; an absent key loads as an empty list.
    #[confer(table)]
    upstream: Vec<Upstream>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let store = Confer::from_string(
        r#"[App]
name = "gateway"
server = { port = 9000 }

[[App.upstream]]
url = "http://10.0.0.1"

[[App.upstream]]
url = "http://10.0.0.2"
weight = 3
"#,
    )?;

    let module = AppSettings::from_confer(store.clone()).await?;
    {
        let guard = module.read().await;
        println!(
            "{} listens on {}:{}",
            guard.name, guard.server.host, guard.server.port
        );
        for upstream in &guard.upstream {
            println!("upstream {} (weight {})", upstream.url, upstream.weight);
        }
    }

    module.write().await.upstream.push(Upstream {
        url: "http://10.0.0.3".into(),
        weight: 2,
    });
    AppSettings::save(&module, store.clone()).await?;
    println!("Updated TOML:\n{}", store.save_str().await?);

    // Errors name the full path of the failing key.
    store
        .load_str("[App]\nname = \"gateway\"\n\n[[App.upstream]]\nurl = \"http://10.0.0.1\"\nweight = -1\n")
        .await?;
    if let Err(error) = AppSettings::load(&module, store.clone()).await {
        println!("Load failed: {error}");
    }

    Ok(())
}
