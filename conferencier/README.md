# conferencier

Async-first, TOML-backed configuration hub with an ergonomic derive macro for mapping strongly typed modules onto shared configuration state.

> Note: This crate is Tokio-backed. All async APIs expect a Tokio runtime, and shared state uses `tokio::sync::RwLock`.

## Highlights

- Shared store powered by Tokio’s `Arc<RwLock<Table>>` for concurrent async access.
- Typed getters/setters covering strings, numbers, booleans, datetimes, and their vector counterparts.
- `#[derive(ConferModule)]` macro to load/save module structs with minimal boilerplate.
- File and in-memory workflows with synchronous and asynchronous loaders.
- Precise error reporting built on `thiserror` for user-friendly diagnostics.

## Quick start

Add to `Cargo.toml`:

```toml
[dependencies]
conferencier = { version = "0.0.3", features = ["with-derive"] }
```

Define a module struct and derive `ConferModule`:

```rust
use conferencier::{Confer, ConferModule, SharedConferModule};

#[derive(Default, ConferModule)]
#[confer(section = "App")]
struct AppConfig {
    name: String,
    #[confer(default = 8080)]
    port: i64,
    features: Vec<String>,
}

#[tokio::main]
async fn main() -> conferencier::Result<()> {
    let store = Confer::from_string("[App]\nname = \"demo\"\n")?;
    let module: SharedConferModule<AppConfig> = AppConfig::from_confer(store.clone()).await?;

    {
        let mut guard = module.write().await;
        guard.features.push("beta-channel".into());
    }

    AppConfig::save(&module, store).await?;
    Ok(())
}
```

## Examples

Run the example:

```powershell
cargo run --example advanced_usage --features with-derive
```

## Derive attributes

The `#[derive(ConferModule)]` macro supports a few attributes to control how your struct maps to TOML.

- `#[confer(section = "Name")]` on the struct sets the TOML section. If omitted, it defaults to the struct name (with an optional `Confer` prefix stripped, e.g. `ConferApp` → `App`).

- `#[confer(rename = "key")]` on a field overrides the TOML key name.

- `#[confer(default = <expr>)]` provides a value when the key is missing.
    - Scalars: strings (quoted), integers, floats, booleans, RFC 3339 datetimes as strings.
    - Vectors: use array syntax, e.g. `#[confer(default = [1, 2, 3])]`, `#[confer(default = ["a", "b"]) ]`.
    - Works with `Option<T>` and `Option<Vec<T>>`; if no default is given, missing keys become `None`.

- `#[confer(init = "<expr>")]` initializes a field before the first load. Useful for preallocations or derived values. Accepts a raw Rust expression or a string literal containing one.

- `#[confer(ignore)]` excludes a field from both load and save; also useful for fields whose type isn’t supported by the derive (e.g., maps or custom structs), or for runtime-only state that shouldn’t be persisted.

Note: `default` and `init` cannot be combined on the same field.

### Supported field types

- Scalars: `String`, `bool`, signed/unsigned integers (`i8`..`i64`, `isize`, `u8`..`u64`, `usize`), floats (`f32`, `f64`), and `toml::value::Datetime`.
- Containers: plain `T`, `Vec<T>`, `Option<T>`, `Option<Vec<T>>`.

Types outside this set produce a friendly compile error.

### Troubleshooting

- "unsupported field type" → use one of the supported scalar/container combinations above.
- "duplicate TOML key" → two fields map to the same key; rename one via `#[confer(rename = ...)]`.
- "cannot combine default and init" → pick one of the attributes per field.
- Defaults must have the right literal form (e.g., `"string"`, `[1, 2]`, or RFC 3339 for datetimes).

## License

MIT OR Apache-2.0
