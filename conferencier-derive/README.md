# conferencier-derive

Procedural macro crate for `#[derive(ConferModule)]`, used by the `conferencier` crate to generate async load/save implementations from TOML-backed configuration.

## Usage

Add to `Cargo.toml` (end users typically depend on `conferencier` and enable `with-derive`):

```toml
[dependencies]
conferencier = { version = "0.0.1", features = ["with-derive"] }
```

Direct use is possible if you prefer to import the macro explicitly:

```toml
[dependencies]
conferencier-derive = "0.0.1"
```

```rust
use conferencier_derive::ConferModule;

#[derive(ConferModule)]
#[confer(section = "App")]
struct AppConfig {
    name: String,
}
```

## Notes

- This crate contains only the derive macro; runtime APIs live in `conferencier`.
- No Tokio dependency is required here; the generated code targets `conferencier`’s Tokio-backed API.

For detailed attribute reference (`section`, `rename`, `default`, `init`, `ignore`) and supported field types, see the main crate’s README.

## License

MIT OR Apache-2.0
