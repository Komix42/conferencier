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

1. Add the crate to your `Cargo.toml` (the `with-derive` feature providing `#[derive(ConferModule)]` is enabled by default):

   ```toml
   [dependencies]
   conferencier = "0.0.3"
   tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
   ```

2. Define a module struct and derive `ConferModule`:

   ```rust
   use conferencier::{Confer, ConferModule, SharedConferModule};

   #[derive(ConferModule)]
   #[confer(section = "App")]
   struct AppConfig {
       name: String,
       #[confer(default = 8080)]
       port: i64,
       #[confer(default = ["stable"])]
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

       AppConfig::save(&module, store.clone()).await?;
       println!("{}", store.save_str().await?);
       Ok(())
   }
   ```

3. Run your application as usual; the derive macro keeps the TOML store and module state in sync.

## Derive attributes

The `#[derive(ConferModule)]` macro understands a small set of field/struct attributes that control how data is mapped to TOML.

- `#[confer(section = "Name")]` on the struct sets the TOML section name. If omitted, it defaults to the struct name with a leading `Confer` word stripped (e.g., `ConferApp` → `App`, while `ConferenceSettings` stays `ConferenceSettings`).

- `#[confer(rename = "key")]` on a field overrides the TOML key name.

- `#[confer(default = <expr>)]` provides a default when the key is missing.
    - Supported scalar defaults: strings (in quotes), integers and floats (including negative values such as `-1` or `-0.5`), booleans, and RFC 3339 datetimes as strings.
    - Vector defaults use array syntax: `#[confer(default = [1, 2, 3])]` or `#[confer(default = ["a", "b"]) ]`.
    - Works with `Option<T>` and `Option<Vec<T>>` as well; when no default is given, missing keys become `None`.

- `#[confer(init = "<expr>")]` initializes a field before the first load (useful for preallocations or derived values). The expression is evaluated as-is; you can also pass it as a string literal if that’s clearer.

- `#[confer(ignore)]` excludes a field from both load and save.

### Supported field types

- Scalars: `String`, `bool`, signed/unsigned integers (`i8..i64`, `isize`, `u8..u64`, `usize`), floats (`f32`, `f64`), and `Datetime` (`conferencier::Datetime`, a re-export of `toml::value::Datetime`).
- Containers: plain `T`, `Vec<T>`, `Option<T>`, `Option<Vec<T>>`.

If a type falls outside this set, the derive emits a compile error with a friendly message.

Known limitation: a required `Datetime` field (plain type without `default`) does not compile yet, because `Datetime` has no `Default` value to start from. Give it a `#[confer(default = "...")]` or use `Option<Datetime>`.

### TOML compatibility

Conferencier uses `toml` 1.2 with TOML 1.1 support. Prefer `conferencier::toml::{Table, Value}` and `conferencier::Datetime` for types shared with the store; types from `toml` 0.9 are not interchangeable with them.

TOML 1.1 times may omit seconds (for example `07:30`, also accepted as a datetime default). In the new datetime API, `Time::second` and `Time::nanosecond` are optional: `None` means the component was omitted, not explicitly set to zero.

## Examples

The crate includes runnable examples:

- `basic_usage` – loads a single module from an in-memory TOML string, updates it, and saves it back.
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

- Architecture notes: [`docs/RFC-0001_baseline_architecture.md`](docs/RFC-0001_baseline_architecture.md)
- Implementation report: see the `docs/` folder for up-to-date status and rationale.

## License

Licensed under either of

- MIT license (`LICENSE-MIT`)
- Apache License, Version 2.0 (`LICENSE-APACHE`)

at your option.

## Contribution

Issues and pull requests are welcome; see [CONTRIBUTING.md](CONTRIBUTING.md). Please open a discussion if you plan significant changes so we can coordinate the approach.
