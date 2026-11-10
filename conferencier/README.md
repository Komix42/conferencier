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

Add to `Cargo.toml` (the `with-derive` feature providing `#[derive(ConferModule)]` is enabled by default):

```toml
[dependencies]
conferencier = "0.0.4"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Define a module struct and derive `ConferModule`:

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

## Examples

Run the example:

```powershell
cargo run --example advanced_usage --features with-derive
```

## Derive attributes

The `#[derive(ConferModule)]` macro supports a few attributes to control how your struct maps to TOML.

- `#[confer(section = "Name")]` on the struct sets the TOML section. If omitted, it defaults to the struct name, with a leading `Confer` word stripped (e.g. `ConferApp` → `App`, while `ConferenceSettings` stays `ConferenceSettings`).

Every field is either loaded from TOML or ignored by the configuration file.

**Loaded fields** (the default) are read from and written to a TOML key:

- `#[confer(rename = "key")]` on a field overrides the TOML key name.

- `#[confer(default = <expr>)]` provides a value when the key is missing.
    - Scalars: strings (quoted), integers and floats (including negative values such as `-1` or `-0.5`), booleans, RFC 3339 datetimes as strings.
    - Vectors: use array syntax, e.g. `#[confer(default = [1, 2, 3])]`, `#[confer(default = ["a", "b"]) ]`.
    - Works with `Option<T>` and `Option<Vec<T>>`; if no default is given, missing keys become `None`.
    - Without a default, a missing key is an error for plain `T` and `Vec<T>` fields (`ConferError::MissingKey`), including `Datetime`.

**Ignored fields** are never read from or written to TOML. They have no key, so they are not checked for key collisions. Use them for runtime-only state or for types the derive does not support (e.g., maps or custom structs):

- `#[confer(ignore)]` initializes the field with `Default::default()`.

- `#[confer(ignore = <expr>)]` initializes the field with a Rust expression, e.g. `#[confer(ignore = Vec::with_capacity(16))]`.
    - The expression is evaluated once, in `from_confer`, after all loaded fields have been read successfully; ignored fields are initialized in declaration order. `load` and `save` never touch them.
    - It is a plain, synchronous Rust expression (not a string) evaluated in its own closure: it cannot use `store`, other fields or `.await`, and `return` or `?` end the expression, not `from_confer`. A string literal is just a `&str`, so for a `String` field write `String::from("abc")`.
    - The field type does not need to implement `Default`.

`ignore` cannot be combined with `rename` or `default`, because an ignored field has no key.

Migrating from 0.0.3: `#[confer(init = ...)]` was removed. Replace `ignore, init = "<expr>"` and `ignore, default = <expr>` with `ignore = <expr>` (no quotes), drop `rename` from ignored fields, and delete `init` from loaded fields (add `default` only if a missing key should fall back to a value). The repository's `CHANGELOG.md` has the full table.

### Supported field types

- Scalars: `String`, `bool`, signed/unsigned integers (`i8`..`i64`, `isize`, `u8`..`u64`, `usize`), floats (`f32`, `f64`), and `Datetime` (`conferencier::Datetime`, a re-export of `toml::value::Datetime`).
- Containers: plain `T`, `Vec<T>`, `Option<T>`, `Option<Vec<T>>`.

Types outside this set produce a friendly compile error.

### TOML compatibility

Conferencier uses `toml` 1.2 with TOML 1.1 support. Prefer `conferencier::toml::{Table, Value}` and `conferencier::Datetime` for types shared with the store; types from `toml` 0.9 are not interchangeable with them.

TOML 1.1 times may omit seconds (for example `07:30`, also accepted as a datetime default). In the new datetime API, `Time::second` and `Time::nanosecond` are optional: `None` means the component was omitted, not explicitly set to zero.

### Troubleshooting

- "unsupported field type" → use one of the supported scalar/container combinations above.
- "duplicate TOML key" → two loaded fields map to the same key; rename one via `#[confer(rename = ...)]`.
- "`#[confer(init)]` was removed" → for an ignored field write `#[confer(ignore = <expr>)]` (no quotes); on a loaded field delete `init`, and add `#[confer(default = ...)]` only if a missing key should fall back to a value.
- "cannot be combined with `#[confer(ignore)]`" → an ignored field has no TOML key; use `#[confer(ignore = <expr>)]` instead of `default`, and drop `rename`.
- Defaults must have the right literal form (e.g., `"string"`, `[1, 2]`, or RFC 3339 for datetimes).

## Saving configuration

Conferencier is primarily meant for reading configuration. Saving works, but it rewrites the configuration rather than editing it:

- `Confer::save_str` and `Confer::save_file` regenerate the whole document from the stored values. Comments are lost, keys may be reordered, inline tables become `[section.table]` headers, and values are written in canonical notation (for example `0x1F90` becomes `8080` and `'text'` becomes `"text"`).
- A module's `save` owns its section: it removes every key and sub-table in the section that is not mapped to a non-ignored field. A misspelled key such as `prot = 9090` is therefore ignored on load (`port` falls back to its default) and deleted by the next save. Other sections are not changed; keep data that no module owns in a separate section.

If people edit the configuration file by hand, prefer to only load it, and use saving mainly to generate an initial configuration file.

## License

MIT OR Apache-2.0
