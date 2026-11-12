# conferencier-derive

Procedural macro crate for `#[derive(ConfigScope)]` and `#[derive(ConfigTable)]`, used by the `conferencier` crate to generate configuration scopes and conversions for nested TOML tables. Scopes are constructed synchronously with `from_store`; their `load` and `save` methods are async because they acquire the scope's Tokio lock. Plain table conversions are synchronous.

## Usage

End users typically depend only on `conferencier`, which re-exports both macros through its `derive` feature, enabled by default:

```toml
[dependencies]
conferencier = "0.0.5"
```

Direct use is possible if you prefer to import the macro explicitly. The generated code always calls into `conferencier`, so that crate stays a dependency (here without its default `derive` feature):

```toml
[dependencies]
conferencier = { version = "0.0.5", default-features = false }
conferencier-derive = "0.0.5"
```

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

For the detailed [attribute reference](https://docs.rs/conferencier#derive-attributes) (`section`, `rename`, `default`, `ignore`, `table`) and supported field types, see the main crate's documentation.

## License

MIT OR Apache-2.0
