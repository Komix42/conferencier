# conferencier-derive

[![crates.io](https://img.shields.io/crates/v/conferencier-derive.svg)](https://crates.io/crates/conferencier-derive)
[![docs.rs](https://img.shields.io/docsrs/conferencier-derive)](https://docs.rs/conferencier-derive)
[![CI](https://github.com/Komix42/conferencier/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Komix42/conferencier/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue)](https://github.com/Komix42/conferencier/blob/main/Cargo.toml)
[![License](https://img.shields.io/crates/l/conferencier-derive.svg)](https://github.com/Komix42/conferencier#license)

Procedural macro crate for `#[derive(ConfigScope)]` and `#[derive(ConfigTable)]`, used by the `conferencier` crate to generate configuration scopes and conversions for nested TOML tables. Scopes are constructed synchronously with `from_store`; their `load` and `save` methods are async because they acquire the scope's Tokio lock. Plain table conversions are synchronous.

## Usage

End users typically depend only on `conferencier`, which re-exports both macros through its `derive` feature, enabled by default:

```toml
[dependencies]
conferencier = "0.1"
```

Direct use is possible if you prefer to import the macro explicitly. The generated code always calls into `conferencier`, so that crate stays a dependency (here without its default `derive` feature):

```toml
[dependencies]
conferencier = { version = "=0.1.0", default-features = false }
conferencier-derive = "=0.1.0"
```

Keep both direct dependencies on the exact same version and update them
together. The default-feature re-export from `conferencier` is the recommended
setup for most users.

```rust
use conferencier_derive::{ConfigScope, ConfigTable};

#[derive(ConfigTable)]
struct Server {
    port: u16,
}

#[derive(ConfigScope)]
#[confer(section = "App")]
struct AppConfig {
    name: String,
    // Mapped to the nested table `[App.server]`.
    #[confer(table)]
    server: Server,
}
```

## Notes

- This crate contains only the derive macros; runtime APIs, including the `ConfigScope` and `ConfigTable` traits, live in `conferencier`.
- No Tokio dependency is required here; the generated code targets `conferencier`’s Tokio-backed API.
- `#[confer(...)]` is the crate-specific derive helper attribute, an abbreviation of *conferencier*, not a struct-name prefix.
- A scope defaults to the full struct name as its TOML section. Through 0.1.x, an affected legacy name such as `ConferApp` requires `section = "App"` to keep the old section or `section = "ConferApp"` to adopt the full name. The migration check is removed in 0.2.0 and does not apply to a table-only derive.
- Scopes and stores are separate values: neither derive starts a background task or performs file I/O. A scope's `save` replaces its whole section, including nested tables and unknown keys.

For the detailed [attribute reference](https://docs.rs/conferencier/%5E0.1.0/conferencier/guide/derive/index.html) (`section`, `rename`, `default`, `ignore`, `table`, `field_defaults`) and supported field types, see the main crate's documentation.
For compatibility and MSRV information, see the [main README](https://github.com/Komix42/conferencier#status-and-compatibility).

## License

Licensed under either of [Apache License, Version 2.0](https://github.com/Komix42/conferencier/blob/main/LICENSE-APACHE) or [MIT license](https://github.com/Komix42/conferencier/blob/main/LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
