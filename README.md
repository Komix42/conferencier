# conferencier

[![crates.io](https://img.shields.io/crates/v/conferencier.svg)](https://crates.io/crates/conferencier)
[![docs.rs](https://img.shields.io/docsrs/conferencier)](https://docs.rs/conferencier)
[![CI](https://github.com/Komix42/conferencier/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Komix42/conferencier/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue)](https://github.com/Komix42/conferencier/blob/main/Cargo.toml)
[![License](https://img.shields.io/crates/l/conferencier.svg)](https://github.com/Komix42/conferencier#license)

**Minimal, easy-to-use typed TOML configuration for Tokio applications.** Describe each application component's settings as a Rust struct, load them from TOML, and share them safely across async tasks. A derive and a few calls are all you need; there is no framework to learn and no hidden machinery.

> Conferencier is in early development (0.0.x). Releases may contain breaking changes; the [migration guide](https://docs.rs/conferencier/latest/conferencier/guide/migration/index.html) describes how to upgrade.

Conferencier keeps configuration explicit and predictable:

- Derives map structs to TOML sections, nested tables and arrays of tables.
- Rust types, defaults and optional fields make configuration visible in application code.
- A store (`ConfigStore`) holds the TOML document; each scope (a struct implementing `ConfigScope`) copies data to or from it only when asked. Nothing reloads or saves in the background.
- Shared scopes (`SharedScope<T>`) use Tokio locks, while the store's in-memory operations are synchronous.

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
    #[confer(ignore = 0)]
    request_count: u64,
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

`request_count` is runtime state: it starts at zero and is never read from or written to TOML. A real application would load its configuration file with `ConfigStore::from_file(path).await?` instead of `from_str`.

Add `conferencier` and Tokio to your project:

```sh
cargo add conferencier tokio --features tokio/macros,tokio/rt-multi-thread
```

or to your `Cargo.toml`:

```toml
[dependencies]
conferencier = "0.0.7"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Typical use

1. The application loads the user's TOML file into a `ConfigStore` and passes clones of this handle to its components.
2. Each component owns one or more sections. It describes each section with a struct deriving `ConfigScope`, independent of the other components. Besides loaded settings, the struct can hold runtime fields such as counters, caches or client handles, which are never read from or written to TOML.
3. The component creates its `SharedScope` with `from_store` and passes clones to its Tokio tasks. The tasks read and change the settings and share the runtime state through it.
4. To persist changes, the component saves its scope back to the store, which replaces only its section. Saving the store to a file writes the whole document, including the sections other components have saved.
5. The file can be the original or another one. For example, an application can keep a hand-edited, commented TOML file as the out-of-the-box configuration and save its effective configuration to a separate file, which it loads instead when it exists. Conferencier does not choose between them; the application does.

## Project scope

Conferencier supports TOML and uses Tokio for async operations. It does not layer configuration sources, watch files or validate application-specific rules. Saving a scope replaces its section; saving the store does not preserve comments or formatting. See [saving configuration](https://docs.rs/conferencier/latest/conferencier/guide/files/index.html) for details.

## Learn more

- [Guide and API reference](https://docs.rs/conferencier/latest/conferencier/), including [concepts](https://docs.rs/conferencier/latest/conferencier/guide/concepts/index.html), the [derive attributes](https://docs.rs/conferencier/latest/conferencier/guide/derive/index.html), and [migration guides](https://docs.rs/conferencier/latest/conferencier/guide/migration/index.html).
- [Examples](https://github.com/Komix42/conferencier/tree/main/conferencier/examples) in the repository.
- [Changelog](https://github.com/Komix42/conferencier/blob/main/CHANGELOG.md) for release history.
- [Contributing](https://github.com/Komix42/conferencier/blob/main/CONTRIBUTING.md) and [issue tracker](https://github.com/Komix42/conferencier/issues).

## License

Licensed under either of [Apache License, Version 2.0](https://github.com/Komix42/conferencier/blob/main/LICENSE-APACHE) or [MIT license](https://github.com/Komix42/conferencier/blob/main/LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
