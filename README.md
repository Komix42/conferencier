# conferencier

Async-first, TOML-backed configuration hub with an ergonomic derive macro for mapping strongly typed modules onto shared configuration state.

## Highlights

- **Shared store** powered by Tokio’s `Arc<RwLock<Table>>` for concurrent async access.
- **Typed getters/setters** covering strings, numbers, booleans, datetimes, and their vector counterparts.
- **`#[derive(ConferModule)]` macro** to load/save module structs with minimal boilerplate.
- **File and in-memory workflows** with synchronous and asynchronous loaders.
- **Precise error reporting** built on `thiserror` for user-friendly diagnostics.

> ⚡ Conferencier is built on the [Tokio](https://tokio.rs/) async runtime. All async APIs expect to run inside a Tokio executor and the shared state uses `tokio::sync::RwLock` under the hood.

## Quick start

1. Add the crates to your `Cargo.toml`. Until the crates are published, depend on the local path:

   ```toml
   [dependencies]
   conferencier = { path = "../conferencier", features = ["with-derive"] }
   conferencier-derive = { path = "../conferencier-derive" }
   ```

   Once released on crates.io you can switch to the semantic version:

   ```toml
   [dependencies]
   conferencier = { version = "0.0.1", features = ["with-derive"] }
   ```

2. Define a module struct and derive `ConferModule`:

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

3. Run your application as usual; the derive macro keeps the TOML store and module state in sync.

## Examples

The crate includes runnable examples:

- `advanced_usage` – demonstrates loading from disk, applying updates, and persisting changes atomically.

Run an example with:

```powershell
cargo run --example advanced_usage --features with-derive
```

## Testing

Execute the full test suite (unit + integration) with:

```powershell
cargo test --all-features
```

## Documentation

- Architecture notes: `docs/RFC-0001_architecture.md`
- Implementation report: see the `docs/` folder for up-to-date status and rationale.

## License

Licensed under either of

- MIT license (`LICENSE-MIT`)
- Apache License, Version 2.0 (`LICENSE-APACHE`)

at your option.

## Contribution

Issues and pull requests are welcome once the project is public. Please open a discussion if you plan significant changes so we can coordinate the approach.
