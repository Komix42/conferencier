# conferencier

[![crates.io](https://img.shields.io/crates/v/conferencier.svg)](https://crates.io/crates/conferencier)
[![docs.rs](https://img.shields.io/docsrs/conferencier)](https://docs.rs/conferencier)
[![CI](https://github.com/Komix42/conferencier/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Komix42/conferencier/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue)](https://github.com/Komix42/conferencier/blob/main/Cargo.toml)
[![License](https://img.shields.io/crates/l/conferencier.svg)](https://github.com/Komix42/conferencier#license)

**Minimal, easy-to-use typed TOML configuration for Tokio applications.** Describe each application component's settings as a Rust struct, load them from TOML, and share them safely across async tasks. A derive and a few calls are all you need; there is no framework to learn and no hidden machinery.

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

    {
        let config = config.read().await;
        println!("HTTP server configured for {}:{}", config.address, config.port);
    }

    // Update runtime state under the write lock and release it right away.
    {
        let mut config = config.write().await;
        config.request_count += 1;
    }

    println!("Requests handled: {}", config.read().await.request_count);
    Ok(())
}
```

`request_count` is runtime state: it starts at zero and is never read from or written to TOML. Hold a scope guard only for a short block; a single write guard keeps several related changes consistent for other readers. A real application would load its configuration file with `ConfigStore::from_file(path).await?` instead of `from_str`.

Add `conferencier` and Tokio to your project:

```sh
cargo add conferencier@0.1
cargo add tokio --features macros,rt-multi-thread
```

or to your `Cargo.toml`:

```toml
[dependencies]
conferencier = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Typical use

1. The application loads the user's TOML file into a `ConfigStore` and passes clones of this handle to its components.
2. Each component owns one or more sections. It describes each section with a struct deriving `ConfigScope`, independent of the other components. Besides loaded settings, the struct can hold runtime fields such as counters, caches or client handles, which are never read from or written to TOML.
3. The component creates its `SharedScope` with `from_store` and passes clones to its Tokio tasks. The tasks read and change the settings and share the runtime state through it.
4. To persist changes, the component saves its scope back to the store, which replaces only its section. Saving the store to a file writes the whole document, including the sections other components have saved.
5. The file can be the original or another one. For example, an application can keep a hand-edited, commented TOML file as the out-of-the-box configuration and save its effective configuration to a separate file, which it loads instead when it exists. Conferencier does not choose between them; the application does.

## Project scope

Conferencier supports TOML and uses Tokio for async operations. It does not layer configuration sources, watch files or validate application-specific rules. Saving a scope replaces its section; saving the store does not preserve comments or formatting. See [saving configuration](https://docs.rs/conferencier/%5E0.1.0/conferencier/guide/files/index.html) for details.

## Status and compatibility

The 0.1.x API was introduced in `0.1.0-beta.1`. Later 0.1.x versions remain
backward compatible with it; breaking changes are deferred to `0.2.0`.
Rust 1.85 is supported throughout 0.1.x. When upgrading from 0.0.x, see the
[migration guide](https://docs.rs/conferencier/%5E0.1.0/conferencier/guide/migration/index.html).
The workspace uses Cargo resolver 3, which prefers dependency versions
compatible with Rust 1.85 when resolving a fresh lockfile.

If your project targets Rust 1.85, declare `rust-version = "1.85"` for its
packages and use Cargo resolver 3 (the default for an edition 2024 root
package; virtual workspaces must set it explicitly), or configure
`resolver.incompatible-rust-versions = "fallback"` in `.cargo/config.toml`.
Resolution prefers compatible versions but cannot guarantee that one exists.

## Learn more

- [Guide and API reference](https://docs.rs/conferencier/%5E0.1.0/conferencier/), including [concepts](https://docs.rs/conferencier/%5E0.1.0/conferencier/guide/concepts/index.html), the [derive attributes](https://docs.rs/conferencier/%5E0.1.0/conferencier/guide/derive/index.html), and [migration guides](https://docs.rs/conferencier/%5E0.1.0/conferencier/guide/migration/index.html).
- [Examples](https://github.com/Komix42/conferencier/tree/main/conferencier/examples) in the repository, and three runnable applications: an [uptime monitor](https://github.com/Komix42/conferencier/tree/main/examples/uptime-monitor) with configuration reload, a [folder indexer](https://github.com/Komix42/conferencier/tree/main/examples/folder-indexer) that manages its own configuration, and a [modular agent](https://github.com/Komix42/conferencier/tree/main/examples/modular-agent) whose independent components own their configuration.
- [Changelog](https://github.com/Komix42/conferencier/blob/main/CHANGELOG.md) for release history.
- [Contributing](https://github.com/Komix42/conferencier/blob/main/CONTRIBUTING.md) and [issue tracker](https://github.com/Komix42/conferencier/issues).

## License

Licensed under either of [Apache License, Version 2.0](https://github.com/Komix42/conferencier/blob/main/LICENSE-APACHE) or [MIT license](https://github.com/Komix42/conferencier/blob/main/LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
