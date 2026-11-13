# conferencier

A minimal, easy-to-use TOML configuration library for Tokio applications. Each part of an application describes its configuration section as a typed Rust struct; conferencier loads the struct from TOML and lets async tasks share it.

A **scope** is one component's configuration and runtime state, shared through a `SharedScope<T>` handle. Its loaded fields own one top-level TOML section; its ignored fields hold state that stays out of the file. The store and each scope are separate values, synchronized only by explicit `load` and `save` calls.

The crate does this one job and deliberately stays small instead of becoming a general-purpose configuration framework (see [Scope](#scope)).

## Highlights

- **Typed scopes.** `#[derive(ConfigScope)]` maps a struct to one top-level TOML section, with defaults, optional values, renamed keys and runtime-only fields.
- **Nested tables** and arrays of tables map to plain structs with `#[derive(ConfigTable)]`.
- **Shared across async tasks.** Scopes are cheap-to-clone handles built on `Arc` and `tokio::sync::RwLock`. The store is a cheap-to-clone handle too: its clones share one document, its in-memory methods are synchronous and only file I/O is async.
- **Nothing runs in the background.** `from_store`, `load` and `save` copy data between a scope and the store only when called, and the store reads or writes files only on request.
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

1. Add the crate to your `Cargo.toml` (the `derive` feature providing both derive macros is enabled by default):

   ```toml
   [dependencies]
   conferencier = "0.0.5"
   tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
   ```

2. Define a scope struct and derive `ConfigScope`:

   ```rust
   use conferencier::{ConfigStore, ConfigScope, SharedScope};

   #[derive(ConfigScope)]
   #[confer(section = "App")]
   struct AppConfig {
       name: String,
       #[confer(default = 8080)]
       port: i64,
       #[confer(default = vec!["stable".into()])]
       features: Vec<String>,
   }

   #[tokio::main]
   async fn main() -> conferencier::Result<()> {
       let store = ConfigStore::from_str("[App]\nname = \"demo\"\n")?;
       let scope: SharedScope<AppConfig> = AppConfig::from_store(&store)?;

       {
           let mut guard = scope.write().await;
           guard.features.push("beta-channel".into());
       }

       AppConfig::save(&scope, &store).await?;
       println!("{}", store.save_str()?);
       Ok(())
   }
   ```

3. Nothing is synchronized in the background: `load` refreshes a scope from the store and `save` writes it into the store. `ConfigStore::save_file` writes the store to a file, and `ConfigStore::load_file` replaces the store with the content of a file (see [Reloading configuration](#reloading-configuration)).

### Features

The default `derive` feature re-exports `#[derive(ConfigScope)]` and `#[derive(ConfigTable)]` alongside the traits. Disable it with `conferencier = { version = "0.0.5", default-features = false }` when you only need store access or manual trait implementations. `ConfigStore`, both traits, `SharedScope`, `Error`, `Result` and the TOML types remain available; Tokio is still used for file I/O and scope locks.

Migrating from 0.0.4: the feature formerly named `with-derive` is now `derive`. The [changelog](CHANGELOG.md) lists the API and signature changes.

## Derive attributes

The `#[derive(ConfigScope)]` macro understands a small set of field/struct attributes that control how data is mapped to TOML. `#[derive(ConfigTable)]` accepts the same field attributes (see [Nested tables](#nested-tables)).

`#[confer(...)]` is the crate-specific derive helper attribute: `confer` abbreviates *conferencier*, like `serde` in `#[serde(...)]`. It is not a required prefix for struct names, and the attribute is not renamed with the public types.

- `#[confer(section = "Name")]` on the struct sets the TOML section name. If omitted, the section is the full struct name: `AppConfig` owns `[AppConfig]` (a raw identifier such as `r#AppConfig` too).
    - Until version 0.0.4, a leading `Confer` word was stripped from the default (`ConferApp` → `[App]`). To keep existing configuration files from silently moving to another section, a `ConfigScope` named `Confer` followed by an uppercase letter, such as `ConferApp`, does not compile without an explicit section: write `#[confer(section = "App")]` to keep the old section or `#[confer(section = "ConferApp")]` to use the full name. Names such as `ConferenceSettings` are not affected.
    - This migration check stays through the 0.1.x releases and is removed in 0.2.0; then `ConferApp` without `section` simply uses `[ConferApp]`.
- `#[confer(deny_unknown_keys)]` on a struct rejects the smallest unknown key in that struct's table when loading. Loaded fields define the known keys after `rename`; ignored fields do not. Strictness is per type, not inherited: if `App` has a lenient nested `Server`, `typo = 1` in `[App.server]` is still accepted; put `deny_unknown_keys` on `Server` too to reject it.

Strictness checks only the section or table being loaded. For example, a strict scope for `[App]` does not reject a misspelled top-level `[Ap]` section; a missing `[App]` is treated as empty and may load if its fields have defaults. Strictness is checked on load, not save; saving still replaces the scope's section and removes keys that do not map to loaded fields.

Every field is either loaded from TOML or ignored by the configuration file.

**Loaded fields** (the default) are read from and written to a TOML key:

- `#[confer(rename = "key")]` on a field overrides the TOML key name.

- `#[confer(default)]` uses the field type's `Default::default()` when its key is missing.

- `#[confer(default = <expr>)]` uses a Rust expression of exactly the field's type when its key is missing. For example, use `"localhost".into()` for `String`, `1.0` for `f64`, `vec!["a".into(), "b".into()]` for `Vec<String>`, and `"07:30".parse().unwrap()` for `Datetime`. An invalid datetime string then panics when the missing-key fallback is evaluated.
    - Defaults run in field declaration order, only when their key is missing; a present but invalid value returns an error instead. A failed load leaves the scope unchanged, but earlier expression side effects are not undone.
    - A syntactically recognized `Option<T>` or `Option<Vec<T>>` cannot have a `default`. Type aliases are opaque to the derive.

- `#[confer(table)]` maps the field to a nested table or an array of tables; see [Nested tables](#nested-tables).

- `#[confer(table, field_defaults)]` loads a missing single table from an empty table, applying its fields' defaults and reporting any required inner key. The table type need not implement `Default`. It may be combined with `rename`, but not with `default`, `ignore`, `Option<T>` or `Vec<T>`.

For every loaded field, a missing key uses its explicit `default` or `field_defaults`, otherwise `None` for a syntactically recognized `Option`, or else `Error::MissingKey`.

**Ignored fields** are never read from or written to TOML. They have no key, so they are not checked for key collisions. Use them for runtime-only state (see [Configuration and runtime state](#configuration-and-runtime-state)) or for types the derive does not support (e.g., maps):

- `#[confer(ignore)]` initializes the field with `Default::default()`.

- `#[confer(ignore = <expr>)]` initializes the field with a Rust expression, e.g. `#[confer(ignore = Vec::with_capacity(16))]`.
    - An ignored field of a scope is initialized once, in `from_store`, after all loaded fields have converted; scope `load` and `save` never touch it. An ignored field of a table is initialized whenever the table is constructed.
    - The field type does not need to implement `Default`.

Both expression forms use a synchronous closure returning the field's type. They may use constants and functions in scope, but not generated parameters, other fields or `.await`. `return` exits the closure, and `?` uses its return type rather than propagating through the generated method.

`ignore` cannot be combined with `rename` or `default`, because an ignored field has no key.

Migrating from 0.0.3: `#[confer(init = ...)]` was removed. Replace `ignore, init = "<expr>"` and `ignore, default = <literal>` with `ignore = <expr>` (no quotes), drop `rename` from ignored fields, and delete `init` from loaded fields (add `default` only if a missing key should fall back to a value). The [changelog](CHANGELOG.md) has the full table.

### Supported field types

- Scalars: `String`, `bool`, signed/unsigned integers (`i8..i64`, `isize`, `u8..u64`, `usize`), floats (`f32`, `f64`), and `Datetime` (`conferencier::Datetime`, a re-export of `toml::value::Datetime`).
- Containers: plain `T`, `Vec<T>`, `Option<T>`, `Option<Vec<T>>`.
- Nested tables: `T`, `Option<T>` and `Vec<T>` where `T` implements `ConfigTable`, with `#[confer(table)]`.

If a type falls outside this set, the derive emits a compile error with a friendly message.
Type aliases are opaque: scalar aliases are unsupported, and table aliases are treated as single table types that must implement `ConfigTable`, even if they name an `Option`.

### TOML compatibility

Conferencier uses `toml` 1.2 with TOML 1.1 support. Prefer `conferencier::toml::{Table, Value}` and `conferencier::Datetime` for types shared with the store; types from `toml` 0.9 are not interchangeable with them.

TOML 1.1 times may omit seconds (for example `07:30`, or `"07:30".parse().unwrap()` as a datetime default). In the new datetime API, `Time::second` and `Time::nanosecond` are optional: `None` means the component was omitted, not explicitly set to zero.

### Troubleshooting

- "unsupported field type" → use one of the supported scalar/container combinations above; for a struct, derive `ConfigTable` on it and add `#[confer(table)]` to the field.
- "cannot be used as a nested configuration table" → the type of a `#[confer(table)]` field must implement `ConfigTable`; derive it, or remove `#[confer(table)]` from a scalar field.
- "duplicate TOML key" → two loaded fields map to the same key; rename one via `#[confer(rename = ...)]`.
- "`ConferApp` needs an explicit section" → choose `#[confer(section = "App")]` to keep the old section, or `#[confer(section = "ConferApp")]` and update the TOML section to use the full name. The compiler cannot check that your choice matches an existing file.
- "`#[confer(init)]` was removed" → for an ignored field write `#[confer(ignore = <expr>)]` (no quotes); on a loaded field delete `init`, and add `#[confer(default = ...)]` only if a missing key should fall back to a value.
- "`unknown key App.prot`" → the containing struct has `#[confer(deny_unknown_keys)]`; fix the key, rename a loaded field to it, or remove strictness if extra keys are intentional. Nested types need the attribute themselves; it is not inherited.
- "cannot be combined with `#[confer(ignore)]`" → an ignored field has no TOML key; use `#[confer(ignore = <expr>)]` instead of `default`, and drop `rename`.
- A default expression must have the field's type: for a `String`, write `"text".into()` rather than `"text"`; for a `Vec`, use `vec![...]` rather than an array literal.
- "`#[confer(default)] cannot be used on an Option field`" → remove the attribute to keep `None` on a missing key, or use the inner type with a default when a value is required.
- "`missing key App.server`" for a formerly implicit table → make it required, add `#[confer(table, field_defaults)]` to decode an empty table, or choose an explicit Rust `default`. Migrate nested table fallbacks too.

## Nested tables

Groups of settings inside a section map to plain structs that derive `ConfigTable`. Mark the field holding them with `#[confer(table)]`:

```rust
use conferencier::{ConfigStore, ConfigScope, ConfigTable};

#[derive(ConfigTable)]
struct Server {
    #[confer(default = "localhost".into())]
    host: String,
    #[confer(default = 8080)]
    port: u16,
}

#[derive(ConfigTable)]
struct Upstream {
    url: String,
    #[confer(default = 1)]
    weight: u32,
}

#[derive(ConfigScope)]
#[confer(section = "App")]
struct AppConfig {
    #[confer(table, field_defaults)]
    server: Server,
    #[confer(table, default)]
    upstream: Vec<Upstream>,
}

#[tokio::main]
async fn main() -> conferencier::Result<()> {
    let store = ConfigStore::from_str(
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
    let scope = AppConfig::from_store(&store)?;

    let config = scope.read().await;
    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.port, 9000);
    assert_eq!(config.upstream[1].weight, 3);
    Ok(())
}
```

A table type takes the same field attributes as a scope (`rename`, `default`, `ignore`, `table` and `field_defaults`), so tables can be nested to any depth. A table field has one of these shapes, where `T` implements `ConfigTable`:

| Field type | TOML | Key missing on load | Save |
| --- | --- | --- | --- |
| `T` | table: `[App.server]` or `server = { ... }` | explicit `default` or `field_defaults`; otherwise `MissingKey` | always written |
| `Option<T>` | table | `None` | `None` removes the key |
| `Vec<T>` | array of tables: `[[App.upstream]]` or `upstream = [{ ... }]` | explicit `default`; otherwise `MissingKey` | always written, including as `upstream = []` |

- An empty `Vec<T>` is saved as `upstream = []`; TOML does not allow adding `[[App.upstream]]` entries after that array, so remove or replace it first.
- `#[confer(table, default)]` calls the table field's Rust `Default`, not its fields' `confer` defaults. For example, a derived `Default` gives `0` to a `u16` field even if that field has `#[confer(default = 8080)]`. To preserve the old missing-table behavior, use `#[confer(table, field_defaults)]` instead: it decodes an empty table, giving `8080` or reporting a required inner key. Apply the same migration to nested table fields.
- `field_defaults` also applies to manually implemented `ConfigTable`s, which receive an empty table and the nested diagnostic path. Derived strict tables check the empty table before converting its fields.
- `#[confer(table)]` cannot be combined with `ignore`. Other shapes such as `Option<Vec<T>>`, `Box<T>` or maps are not supported.
- Errors keep a formatted table `path` and a logical key; diagnostic output quotes dotted or otherwise non-bare TOML key segments. For example, a missing `a.b` key reports `missing key App."a.b"`. Scalar-array conversion errors include the zero-based element index, e.g. `invalid value at App.ports: value out of range for u16 (at index 1)`.
- Tables are plain values: every successful `load` builds them anew. Ignored fields inside a table type are therefore initialized again on each load (with `Default::default()` or their `ignore = <expr>` initializer), unlike ignored fields of the scope itself. If the load fails, the scope keeps its previous state, but side effects of initializers that have already run are not undone.
- One struct can derive both `ConfigScope` and `ConfigTable`; `section` then applies only to the scope. A table-only derive rejects `section`. `#[derive(ConfigScope)]` alone does not make a struct usable as a table field.
- `#[confer(section = "App.server")]` does not refer to the nested table `[App.server]`: it names a top-level section whose name contains a dot (saved as `["App.server"]`). A nested table always belongs to the scope of its top-level section.
- A generic table field needs an explicit bound such as `where T: ConfigTable`. `ConfigTable` can also be implemented manually; see its documentation. Manual implementations can build the same diagnostic paths as the derive with `conferencier::table_path::{section_path, child_path, element_path}`, which are available without the `derive` feature.
- A strict outer struct does not make its nested table types strict. Add `#[confer(deny_unknown_keys)]` to each nested `ConfigTable` that should reject unknown keys; conversely, a strict table type remains strict wherever it is used.

## Reloading configuration

Conferencier never reloads anything on its own. To apply an edited configuration file, load it into the store and then reload every scope that uses it:

```rust
use std::path::Path;

use conferencier::{ConfigScope, ConfigStore, SharedScope};

async fn reload<S: ConfigScope>(
    store: &ConfigStore,
    scope: &SharedScope<S>,
    path: &Path,
) -> conferencier::Result<()> {
    store.load_file(path).await?;
    S::load(scope, store).await
}
```

- `ConfigStore::load_file` and `ConfigStore::load_str` replace the whole store, including sections that no scope owns and values saved into the store but not yet written to a file. If the new document cannot be read or parsed, the store keeps its previous content.
- `load` updates a scope only if all its loaded fields convert successfully; otherwise it returns the error and the scope keeps its previous values. Ignored fields of the scope keep their values in both cases.
- A scope that is not reloaded keeps its previous values, even though the store has changed.
- Reloading several scopes is not one transaction: if a later scope fails to load, the store and scopes already reloaded keep their new values.

## Configuration and runtime state

A scope can also hold runtime state that does not belong in the configuration file, such as counters, caches or client handles. Mark such fields with `#[confer(ignore)]` or `#[confer(ignore = <expr>)]`: `from_store` initializes them, later `load` calls keep their values, and `save` never writes them. Tasks then share the settings and the state through one `SharedScope<T>` handle (`Arc<RwLock<T>>`); clone the handle to pass it to another task.

```rust
use conferencier::{ConfigStore, ConfigScope};

#[derive(ConfigScope)]
#[confer(section = "Server")]
struct Server {
    port: u16,
    #[confer(ignore)]
    requests_served: u64,
}

#[tokio::main]
async fn main() -> conferencier::Result<()> {
    let store = ConfigStore::from_str("[Server]\nport = 8080\n")?;
    let server = Server::from_store(&store)?;

    let handle = server.clone();
    tokio::spawn(async move {
        handle.write().await.requests_served += 1;
    })
    .await
    .expect("task panicked");

    // A reload updates the port and keeps the counter.
    store.load_str("[Server]\nport = 9090\n")?;
    Server::load(&server, &store).await?;

    let state = server.read().await;
    assert_eq!(state.port, 9090);
    assert_eq!(state.requests_served, 1);
    Ok(())
}
```

Keep state that must survive reloads directly in the scope: ignored fields inside `ConfigTable` values are initialized again on every load (see [Nested tables](#nested-tables)).

The handle's Tokio `RwLock` is not reentrant, so drop every guard of a scope before calling its `load` or `save` in the same task. `load` locks the scope for writing and would wait for a held guard forever. `save` locks it for reading, which waits forever behind a held write guard and, because Tokio's lock is fair, also behind a held read guard while another task is waiting to write. Copy what you need out of a short-lived guard instead, for example `let port = server.read().await.port;`.

## Saving configuration

Conferencier is primarily meant for reading configuration. Saving works, but it rewrites the configuration rather than editing it:

- `ConfigStore::save_str` and `ConfigStore::save_file` regenerate the whole document from the stored values. Comments are lost, keys may be reordered, inline tables become `[section.table]` headers, and values are written in canonical notation (for example `0x1F90` becomes `8080` and `'text'` becomes `"text"`).
- A scope's `save` owns its section, including all tables nested in it: it removes every key and sub-table that is not mapped to a non-ignored field, at any depth (also inside tables mapped with `#[confer(table)]`). In lenient mode, a misspelled key such as `prot = 9090` is ignored on load (`port` falls back to its default) and deleted by the next save. A strict type rejects it on load, but save still replaces the section without rechecking. Other sections are not changed; keep data that no scope owns in a separate section.
- A computed default is saved like any other loaded value; later loads read that value instead of recomputing it. Use an `Option` with an application-level fallback if the value should stay dynamic after saving.
- A well-typed default does not necessarily round-trip: a non-finite `f32` such as `f32::NAN` can be saved but is rejected on load.
- Concurrent `save_file` calls to the same file, even through cloned stores, never share a temporary file: each successful call installs one complete snapshot, but which call wins is unspecified. Serialize saves yourself if their order matters. Replacement and durability limitations are described in the [API reference](https://docs.rs/conferencier/latest/conferencier/struct.ConfigStore.html#method.save_file).

If people edit the configuration file by hand, prefer to only load it, and use saving mainly to generate an initial configuration file.

## Examples

The crate includes runnable examples:

- `basic_usage` – loads a single scope from an in-memory TOML string, updates it, and saves it back.
- `advanced_usage` – demonstrates loading from disk, applying updates, and saving the updated configuration to a file.
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
