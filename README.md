# conferencier

A minimal, single-purpose TOML configuration library for Tokio applications. Each part of an application describes its configuration section as a typed Rust struct; conferencier loads the struct from TOML and lets async tasks share it.

The crate does this one job and deliberately stays small instead of becoming a general-purpose configuration framework (see [Scope](#scope)).

## Highlights

- **Typed modules.** `#[derive(ConferModule)]` maps a struct to one top-level TOML section, with defaults, optional values, renamed keys and runtime-only fields.
- **Nested tables** and arrays of tables map to plain structs with `#[derive(ConferTable)]`.
- **Shared across async tasks.** Modules and the store are cheap-to-clone handles built on `Arc` and `tokio::sync::RwLock`.
- **Nothing runs in the background.** `from_confer`, `load` and `save` copy data between a module and the store only when called, and the store reads or writes files only on request.
- **Errors name the key**, such as `missing key App.name` or `invalid value at App.upstream[1].weight`; mistakes in attributes are compile errors.
- **Typed store access without the derive:** getters and setters for strings, numbers, booleans, datetimes and their vectors.

## Scope

Conferencier is built on [Tokio](https://tokio.rs/): all async APIs expect a Tokio runtime. TOML is the only supported format. The following are intentionally left out:

- layering configuration from environment variables, command-line arguments or several files;
- watching files and reloading configuration automatically;
- validation beyond value types, such as allowed ranges, patterns or rules across fields; check these in your application after loading;
- preserving comments and formatting when saving (see [Saving configuration](#saving-configuration)).

If you need any of these, a general-purpose configuration crate is a better fit.

## Quick start

1. Add the crate to your `Cargo.toml` (the `with-derive` feature providing `#[derive(ConferModule)]` is enabled by default):

   ```toml
   [dependencies]
   conferencier = "0.0.4"
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

3. Nothing is synchronized in the background: `load` refreshes a module from the store and `save` writes it into the store. `Confer::save_file` writes the store to a file, and `Confer::load_file` replaces the store with the content of a file (see [Reloading configuration](#reloading-configuration)).

## Derive attributes

The `#[derive(ConferModule)]` macro understands a small set of field/struct attributes that control how data is mapped to TOML. `#[derive(ConferTable)]` accepts the same field attributes (see [Nested tables](#nested-tables)).

- `#[confer(section = "Name")]` on the struct sets the TOML section name. If omitted, it defaults to the struct name with a leading `Confer` word stripped (e.g., `ConferApp` → `App`, while `ConferenceSettings` stays `ConferenceSettings`).

Every field is either loaded from TOML or ignored by the configuration file.

**Loaded fields** (the default) are read from and written to a TOML key:

- `#[confer(rename = "key")]` on a field overrides the TOML key name.

- `#[confer(default = <expr>)]` provides a default when the key is missing.
    - Supported scalar defaults: strings (in quotes), integers and floats (including negative values such as `-1` or `-0.5`), booleans, and RFC 3339 datetimes as strings.
    - Vector defaults use array syntax: `#[confer(default = [1, 2, 3])]` or `#[confer(default = ["a", "b"]) ]`.
    - Works with `Option<T>` and `Option<Vec<T>>` as well; when no default is given, missing keys become `None`.
    - Without a default, a missing key is an error for plain `T` and `Vec<T>` fields (`ConferError::MissingKey`), including `Datetime`.

- `#[confer(table)]` maps the field to a nested table or an array of tables; see [Nested tables](#nested-tables).

**Ignored fields** are never read from or written to TOML. They have no key, so they are not checked for key collisions. Use them for runtime-only state (see [Configuration and runtime state](#configuration-and-runtime-state)) or for types the derive does not support (e.g., maps):

- `#[confer(ignore)]` initializes the field with `Default::default()`.

- `#[confer(ignore = <expr>)]` initializes the field with a Rust expression, e.g. `#[confer(ignore = Vec::with_capacity(16))]`.
    - The expression is evaluated once, in `from_confer`, after all loaded fields have been read successfully; ignored fields are initialized in declaration order. `load` and `save` never touch them.
    - It is a plain, synchronous Rust expression (not a string) evaluated in its own closure: it cannot use `store`, other fields or `.await`, and `return` or `?` end the expression, not `from_confer`. A string literal is just a `&str`, so for a `String` field write `String::from("abc")`.
    - The field type does not need to implement `Default`.

`ignore` cannot be combined with `rename` or `default`, because an ignored field has no key.

Migrating from 0.0.3: `#[confer(init = ...)]` was removed. Replace `ignore, init = "<expr>"` and `ignore, default = <expr>` with `ignore = <expr>` (no quotes), drop `rename` from ignored fields, and delete `init` from loaded fields (add `default` only if a missing key should fall back to a value). The [changelog](CHANGELOG.md) has the full table.

### Supported field types

- Scalars: `String`, `bool`, signed/unsigned integers (`i8..i64`, `isize`, `u8..u64`, `usize`), floats (`f32`, `f64`), and `Datetime` (`conferencier::Datetime`, a re-export of `toml::value::Datetime`).
- Containers: plain `T`, `Vec<T>`, `Option<T>`, `Option<Vec<T>>`.
- Nested tables: `T`, `Option<T>` and `Vec<T>` where `T` implements `ConferTable`, with `#[confer(table)]`.

If a type falls outside this set, the derive emits a compile error with a friendly message.

### TOML compatibility

Conferencier uses `toml` 1.2 with TOML 1.1 support. Prefer `conferencier::toml::{Table, Value}` and `conferencier::Datetime` for types shared with the store; types from `toml` 0.9 are not interchangeable with them.

TOML 1.1 times may omit seconds (for example `07:30`, also accepted as a datetime default). In the new datetime API, `Time::second` and `Time::nanosecond` are optional: `None` means the component was omitted, not explicitly set to zero.

### Troubleshooting

- "unsupported field type" → use one of the supported scalar/container combinations above; for a struct, derive `ConferTable` on it and add `#[confer(table)]` to the field.
- "cannot be used as a nested configuration table" → the type of a `#[confer(table)]` field must implement `ConferTable`; derive it, or remove `#[confer(table)]` from a scalar field.
- "duplicate TOML key" → two loaded fields map to the same key; rename one via `#[confer(rename = ...)]`.
- "`#[confer(init)]` was removed" → for an ignored field write `#[confer(ignore = <expr>)]` (no quotes); on a loaded field delete `init`, and add `#[confer(default = ...)]` only if a missing key should fall back to a value.
- "cannot be combined with `#[confer(ignore)]`" → an ignored field has no TOML key; use `#[confer(ignore = <expr>)]` instead of `default`, and drop `rename`.
- Defaults must have the right literal form (e.g., `"string"`, `[1, 2]`, or RFC 3339 for datetimes).

## Nested tables

Groups of settings inside a section map to plain structs that derive `ConferTable`. Mark the field holding them with `#[confer(table)]`:

```rust
use conferencier::{Confer, ConferModule, ConferTable};

#[derive(ConferTable)]
struct Server {
    #[confer(default = "localhost")]
    host: String,
    #[confer(default = 8080)]
    port: u16,
}

#[derive(ConferTable)]
struct Upstream {
    url: String,
    #[confer(default = 1)]
    weight: u32,
}

#[derive(ConferModule)]
#[confer(section = "App")]
struct AppConfig {
    #[confer(table)]
    server: Server,
    #[confer(table)]
    upstream: Vec<Upstream>,
}

#[tokio::main]
async fn main() -> conferencier::Result<()> {
    let store = Confer::from_string(
        r#"
[App.server]
port = 9000

[[App.upstream]]
url = "http://10.0.0.1"

[[App.upstream]]
url = "http://10.0.0.2"
weight = 3
"#,
    )?;
    let module = AppConfig::from_confer(store.clone()).await?;

    let config = module.read().await;
    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.port, 9000);
    assert_eq!(config.upstream[1].weight, 3);
    Ok(())
}
```

A table type takes the same field attributes as a module (`rename`, `default`, `ignore` and `table`), so tables can be nested to any depth. A table field has one of these shapes, where `T` implements `ConferTable`:

| Field type | TOML | Key missing on load | Save |
| --- | --- | --- | --- |
| `T` | table: `[App.server]` or `server = { ... }` | loaded from an empty table: the defaults of `T` apply, a required field of `T` fails with `MissingKey` | always written |
| `Option<T>` | table | `None` | `None` removes the key |
| `Vec<T>` | array of tables: `[[App.upstream]]` or `upstream = [{ ... }]` | empty `Vec` | an empty `Vec` removes the key |

- An absent table `Vec<T>` loads as empty, while an absent scalar `Vec` without `default` (e.g. `Vec<String>`) fails with `MissingKey`. An empty `Vec<T>` is not saved as `upstream = []`, because TOML would then reject `[[App.upstream]]` entries added by hand.
- `#[confer(table)]` cannot be combined with `default` or `ignore`; defaults belong to the fields of `T`. Other shapes such as `Option<Vec<T>>`, `Box<T>` or maps are not supported.
- Errors name the full path of the failing key, e.g. `invalid value at App.upstream[1].weight: value out of range for u32`.
- Tables are plain values: every successful `load` builds them anew. Ignored fields inside a table type are therefore initialized again on each load (with `Default::default()` or their `ignore = <expr>` initializer), unlike ignored fields of the module itself. If the load fails, the module keeps its previous state, but side effects of initializers that have already run are not undone.
- One struct can derive both `ConferModule` and `ConferTable`; `section` then applies only to the module. `#[derive(ConferModule)]` alone does not make a struct usable as a table field.
- `#[confer(section = "App.server")]` does not refer to the nested table `[App.server]`: it names a top-level section whose name contains a dot (saved as `["App.server"]`). A nested table always belongs to the module of its top-level section.
- A generic table field needs an explicit bound such as `where T: ConferTable`. `ConferTable` can also be implemented manually; see its documentation.

## Reloading configuration

Conferencier never reloads anything on its own. To apply an edited configuration file, load it into the store and then reload every module that uses it:

```rust
use std::path::Path;

use conferencier::{ConferModule, SharedConfer, SharedConferModule};

async fn reload<M: ConferModule>(
    store: &SharedConfer,
    module: &SharedConferModule<M>,
    path: &Path,
) -> conferencier::Result<()> {
    store.load_file(path).await?;
    M::load(module, store.clone()).await
}
```

- `Confer::load_file` and `Confer::load_str` replace the whole store, including sections that no module owns and values saved into the store but not yet written to a file. If the new document cannot be read or parsed, the store keeps its previous content.
- `load` updates a module only if all its loaded fields convert successfully; otherwise it returns the error and the module keeps its previous values. Ignored fields of the module keep their values in both cases.
- A module that is not reloaded keeps its previous values, even though the store has changed.

## Configuration and runtime state

A module can also hold runtime state that does not belong in the configuration file, such as counters, caches or client handles. Mark such fields with `#[confer(ignore)]` or `#[confer(ignore = <expr>)]`: `from_confer` initializes them, later `load` calls keep their values, and `save` never writes them. Tasks then share the settings and the state through one `SharedConferModule<T>` handle (`Arc<RwLock<T>>`); clone the handle to pass it to another task.

```rust
use conferencier::{Confer, ConferModule};

#[derive(ConferModule)]
#[confer(section = "Server")]
struct Server {
    port: u16,
    #[confer(ignore)]
    requests_served: u64,
}

#[tokio::main]
async fn main() -> conferencier::Result<()> {
    let store = Confer::from_string("[Server]\nport = 8080\n")?;
    let server = Server::from_confer(store.clone()).await?;

    let handle = server.clone();
    tokio::spawn(async move {
        handle.write().await.requests_served += 1;
    })
    .await
    .expect("task panicked");

    // A reload updates the port and keeps the counter.
    store.load_str("[Server]\nport = 9090\n").await?;
    Server::load(&server, store.clone()).await?;

    let state = server.read().await;
    assert_eq!(state.port, 9090);
    assert_eq!(state.requests_served, 1);
    Ok(())
}
```

Keep state that must survive reloads directly in the module: ignored fields inside `ConferTable` values are initialized again on every load (see [Nested tables](#nested-tables)).

The handle's Tokio `RwLock` is not reentrant, so drop every guard of a module before calling its `load` or `save` in the same task. `load` locks the module for writing and would wait for a held guard forever. `save` locks it for reading, which waits forever behind a held write guard and, because Tokio's lock is fair, also behind a held read guard while another task is waiting to write. Copy what you need out of a short-lived guard instead, for example `let port = server.read().await.port;`.

## Saving configuration

Conferencier is primarily meant for reading configuration. Saving works, but it rewrites the configuration rather than editing it:

- `Confer::save_str` and `Confer::save_file` regenerate the whole document from the stored values. Comments are lost, keys may be reordered, inline tables become `[section.table]` headers, and values are written in canonical notation (for example `0x1F90` becomes `8080` and `'text'` becomes `"text"`).
- A module's `save` owns its section, including all tables nested in it: it removes every key and sub-table that is not mapped to a non-ignored field, at any depth (also inside tables mapped with `#[confer(table)]`). A misspelled key such as `prot = 9090` is therefore ignored on load (`port` falls back to its default) and deleted by the next save. Other sections are not changed; keep data that no module owns in a separate section.

If people edit the configuration file by hand, prefer to only load it, and use saving mainly to generate an initial configuration file.

## Examples

The crate includes runnable examples:

- `basic_usage` – loads a single module from an in-memory TOML string, updates it, and saves it back.
- `advanced_usage` – demonstrates loading from disk, applying updates, and persisting changes atomically.
- `nested_tables` – maps a sub-table and an array of tables to plain structs and shows an error inside a nested table.

Run an example with:

```powershell
cargo run --example advanced_usage
```

## Testing

Execute the full test suite of both crates (unit, integration, doc and compile-fail tests) with:

```powershell
cargo test --workspace --all-features
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the remaining checks run in CI.

## Documentation

- [API reference](https://docs.rs/conferencier) of the latest release on docs.rs.
- [CHANGELOG.md](CHANGELOG.md) lists the changes in each release, including migration notes.
- [`docs/`](docs/) keeps the design records (RFCs and implementation reports). They record the design at the time they were written and are not updated later; this README and the API reference describe the current behavior.
- [`docs/BACKLOG.md`](docs/BACKLOG.md) collects known technical debt and ideas for future versions.

## License

Licensed under either of

- MIT license (`LICENSE-MIT`)
- Apache License, Version 2.0 (`LICENSE-APACHE`)

at your option.

## Contribution

Issues and pull requests are welcome; see [CONTRIBUTING.md](CONTRIBUTING.md). Please open a discussion if you plan significant changes so we can coordinate the approach.
