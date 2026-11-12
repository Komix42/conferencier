# conferencier-derive

Procedural macro crate for `#[derive(ConfigScope)]` and `#[derive(ConfigTable)]`, used by the `conferencier` crate to generate async load/save implementations for configuration scopes and conversions for nested TOML tables.

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

For detailed attribute reference (`section`, `rename`, `default`, `ignore`, `table`) and supported field types, see the main crate’s README.

## License

MIT OR Apache-2.0
