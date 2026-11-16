# conferencier

**Typed TOML configuration for Tokio applications.** Describe each application component's settings as a Rust struct, load them from TOML, and share them safely across async tasks.

Conferencier keeps configuration explicit and predictable:

- Derives map structs to TOML sections, nested tables and arrays of tables.
- Rust types, defaults and optional fields make configuration visible in application code.
- A `ConfigStore` holds the TOML document; each scope copies data to or from it only when asked. Nothing reloads or saves in the background.
- Shared scopes use Tokio locks, while the store's in-memory operations are synchronous.

## Example

A web service can keep its listener settings in one typed scope:

```rust
use conferencier::{ConfigScope, ConfigStore, SharedScope};

#[derive(ConfigScope)]
#[confer(section = "Http")]
struct HttpConfig {
    address: String,
    #[confer(default = 8080)]
    port: u16,
}

#[tokio::main]
async fn main() -> conferencier::Result<()> {
    let store = ConfigStore::from_str(
        "[Http]\naddress = \"127.0.0.1\"\n",
    )?;
    let config: SharedScope<HttpConfig> = HttpConfig::from_store(&store)?;

    let config = config.read().await;
    println!("HTTP server configured for {}:{}", config.address, config.port);
    Ok(())
}
```

Add `conferencier` and Tokio to your `Cargo.toml`:

```toml
[dependencies]
conferencier = "0.0.7"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Learn more

- [Guide and API reference](https://docs.rs/conferencier/latest/conferencier/), including [concepts](https://docs.rs/conferencier/latest/conferencier/guide/concepts/index.html), the [derive attributes](https://docs.rs/conferencier/latest/conferencier/guide/derive/index.html), and [migration guides](https://docs.rs/conferencier/latest/conferencier/guide/migration/index.html).
- [Examples](https://github.com/Komix42/conferencier/tree/main/conferencier/examples) in the repository.
- [Changelog](https://github.com/Komix42/conferencier/blob/main/CHANGELOG.md) for release history.
- [Contributing](https://github.com/Komix42/conferencier/blob/main/CONTRIBUTING.md) and [issue tracker](https://github.com/Komix42/conferencier/issues).

Licensed under either [MIT](https://github.com/Komix42/conferencier/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/Komix42/conferencier/blob/main/LICENSE-APACHE).
