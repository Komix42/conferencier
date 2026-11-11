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

Nothing is synchronized in the background: `load` refreshes a module from the store and `save` writes it back; `Confer::load_file` and `Confer::save_file` do the same between the store and a file.

## Examples

Run an example:

```powershell
cargo run --example advanced_usage --features with-derive
cargo run --example nested_tables --features with-derive
```

## Derive attributes

The `#[derive(ConferModule)]` macro supports a few attributes to control how your struct maps to TOML. `#[derive(ConferTable)]` accepts the same field attributes (see [Nested tables](#nested-tables)).

- `#[confer(section = "Name")]` on the struct sets the TOML section. If omitted, it defaults to the struct name, with a leading `Confer` word stripped (e.g. `ConferApp` → `App`, while `ConferenceSettings` stays `ConferenceSettings`).

Every field is either loaded from TOML or ignored by the configuration file.

**Loaded fields** (the default) are read from and written to a TOML key:

- `#[confer(rename = "key")]` on a field overrides the TOML key name.

- `#[confer(default = <expr>)]` provides a value when the key is missing.
    - Scalars: strings (quoted), integers and floats (including negative values such as `-1` or `-0.5`), booleans, RFC 3339 datetimes as strings.
    - Vectors: use array syntax, e.g. `#[confer(default = [1, 2, 3])]`, `#[confer(default = ["a", "b"]) ]`.
    - Works with `Option<T>` and `Option<Vec<T>>`; if no default is given, missing keys become `None`.
    - Without a default, a missing key is an error for plain `T` and `Vec<T>` fields (`ConferError::MissingKey`), including `Datetime`.

- `#[confer(table)]` maps the field to a nested table or an array of tables; see [Nested tables](#nested-tables).

**Ignored fields** are never read from or written to TOML. They have no key, so they are not checked for key collisions. Use them for runtime-only state or for types the derive does not support (e.g., maps):

- `#[confer(ignore)]` initializes the field with `Default::default()`.

- `#[confer(ignore = <expr>)]` initializes the field with a Rust expression, e.g. `#[confer(ignore = Vec::with_capacity(16))]`.
    - The expression is evaluated once, in `from_confer`, after all loaded fields have been read successfully; ignored fields are initialized in declaration order. `load` and `save` never touch them.
    - It is a plain, synchronous Rust expression (not a string) evaluated in its own closure: it cannot use `store`, other fields or `.await`, and `return` or `?` end the expression, not `from_confer`. A string literal is just a `&str`, so for a `String` field write `String::from("abc")`.
    - The field type does not need to implement `Default`.

`ignore` cannot be combined with `rename` or `default`, because an ignored field has no key.

Migrating from 0.0.3: `#[confer(init = ...)]` was removed. Replace `ignore, init = "<expr>"` and `ignore, default = <expr>` with `ignore = <expr>` (no quotes), drop `rename` from ignored fields, and delete `init` from loaded fields (add `default` only if a missing key should fall back to a value). The repository's `CHANGELOG.md` has the full table.

### Sharing configuration and runtime state

A `ConferModule` can hold both settings loaded from TOML and runtime state that should not be persisted. The module is shared as a `SharedConferModule<T>` (`Arc<RwLock<T>>`), so application tasks can read or update both kinds of fields through the same handle. An ignored module field is initialized by `from_confer`, left untouched by later `load` calls, and never written by `save`.

```rust
use conferencier::{ConferModule, SharedConferModule};

#[derive(ConferModule)]
#[confer(section = "Server")]
#[allow(dead_code)]
struct Server {
    port: u16,
    #[confer(ignore)]
    requests_served: u64,
}

#[allow(dead_code)]
async fn record_request(server: &SharedConferModule<Server>) {
    server.write().await.requests_served += 1;
}
```

Clone the `SharedConferModule` handle to share this state with other tasks. Put persistent fields in the module itself: ignored fields inside `ConferTable` values are initialized again whenever those tables are loaded.

### Supported field types

- Scalars: `String`, `bool`, signed/unsigned integers (`i8`..`i64`, `isize`, `u8`..`u64`, `usize`), floats (`f32`, `f64`), and `Datetime` (`conferencier::Datetime`, a re-export of `toml::value::Datetime`).
- Containers: plain `T`, `Vec<T>`, `Option<T>`, `Option<Vec<T>>`.
- Nested tables: `T`, `Option<T>` and `Vec<T>` where `T` implements `ConferTable`, with `#[confer(table)]`.

Types outside this set produce a friendly compile error.

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

## Saving configuration

Conferencier is primarily meant for reading configuration. Saving works, but it rewrites the configuration rather than editing it:

- `Confer::save_str` and `Confer::save_file` regenerate the whole document from the stored values. Comments are lost, keys may be reordered, inline tables become `[section.table]` headers, and values are written in canonical notation (for example `0x1F90` becomes `8080` and `'text'` becomes `"text"`).
- A module's `save` owns its section, including all tables nested in it: it removes every key and sub-table that is not mapped to a non-ignored field, at any depth (also inside tables mapped with `#[confer(table)]`). A misspelled key such as `prot = 9090` is therefore ignored on load (`port` falls back to its default) and deleted by the next save. Other sections are not changed; keep data that no module owns in a separate section.

If people edit the configuration file by hand, prefer to only load it, and use saving mainly to generate an initial configuration file.

## License

MIT OR Apache-2.0
