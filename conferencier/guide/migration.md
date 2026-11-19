How to upgrade code and configuration files from earlier releases.

For an upgrade spanning several releases, follow each applicable section in
order. Names in a section refer to the API of that release; for example,
`ConferModule` in the 0.0.4 instructions becomes `ConfigScope` in 0.0.5.
Before saving, check that each scope uses the intended TOML section.

## From 0.0.8 to 0.1.0-beta.1

This is the first 0.1.x release and includes breaking API changes. Check
downstream code that constructs or matches errors, uses typed setters, or relies
on Tokio features enabled transitively.

### Errors

- `Error::Io`, `Parse` and `Serialize` no longer repeat their source in
  `Display`. `SaveFileCleanup` still displays its cleanup error, while
  `source()` exposes the original save error. Use `source()` or an error-chain
  reporter when the cause is needed. Do not parse diagnostic text or rely on
  its exact wording; inspect variants, fields and sources instead. The
  [troubleshooting guide](crate::guide::troubleshooting#showing-error-causes)
  has an opt-in helper for displaying a full chain.
- `Error::Io` is non-exhaustive and adds `operation` and `target_path`.
  Replace direct construction with `Error::io_error` or
  `Error::io_error_with_operation`, and include `..` in patterns.
- `Error::Parse` and `Error::Serialize` are now non-exhaustive struct variants.
  Replace tuple patterns such as `Error::Parse(_)` with
  `Error::Parse { source, .. }`; use `Error::parse_error` or
  `Error::serialize_error` when constructing them. `Parse.path` identifies the
  parsed file; `Serialize.target_path` identifies a `save_file` destination.
- `Error::SaveFileCleanup` is non-exhaustive, adds `target_path`, and has no
  public constructor. Inspect the library-created variant with `..`.
  `save_file` task failures now return `Error::SaveFileTaskJoin`, not
  `Error::Io`; handle that variant where the distinction matters.
- `MissingKey`, `TypeMismatch`, `ValueParse` and `UnknownKey` are non-exhaustive.
  The key-error field `path` is renamed to `table_path`; filesystem `path`
  fields are unchanged. Include `..` in key-error patterns. The constructors
  are renamed with an `_error` suffix, with the same argument types and order
  and no deprecated aliases.

For manual `ConfigTable` implementations, rename key-error constructor calls
and use the table path when building them. The method parameter's identifier is
local and may keep any name; the example shows the recommended name:

```rust,ignore
// 0.0.8
impl ConfigTable for Settings {
    fn from_table(path: &str, _table: Table) -> Result<Self> {
        Err(Error::missing_key(path, "port"))
    }

    fn to_table(&self, _path: &str) -> Result<Table> {
        Ok(Table::new())
    }
}
```

**After (0.1.0-beta.1):**

```rust,ignore
impl ConfigTable for Settings {
    fn from_table(table_path: &str, _table: Table) -> Result<Self> {
        Err(Error::missing_key_error(table_path, "port"))
    }

    fn to_table(&self, _table_path: &str) -> Result<Table> {
        Ok(Table::new())
    }
}
```

### Tokio, setters and `Debug`

- The library no longer enables Tokio's `macros` or `rt-multi-thread` features.
  Add the features your application uses to its own Tokio dependency.
- Applications that import macros directly from `conferencier-derive` must
  pin it to the exact same version as `conferencier`; the default-feature
  re-export from `conferencier` is the recommended setup.
- `set_string` accepts `impl Into<String>` and vector setters accept
  `IntoIterator`. Pass values directly instead of relying on an ambiguous
  `.into()`, and pass iterators directly instead of collecting them without a
  type annotation. Empty string vectors need an element type, such as
  `Vec::<String>::new()`.
- `ConfigStore`'s `Debug` output no longer contains configuration values.
  Its text format is not stable; do not parse it.

### Non-breaking behavior changes

- Scalar and vector `f32` fields now load infinities and NaNs. Validate values
  in the application if non-finite values must be rejected; NaN sign and
  payload are not preserved.
- Save-time range errors for `u64` and `usize` no longer include the numeric
  value. Vector errors still report the failing element index.

### Existing behavior, not a source migration

`from_file`, `load_file` and `save_file` still require an active Tokio runtime
when their file operation reaches Tokio; polling them outside a runtime can
panic. `save_file` returns a serialization error before it needs a runtime.
Caller-supplied code may have its own runtime requirements. With the default
TOML features, deeply nested untrusted input is rejected with a parse error;
enabling `toml/unbounded` removes that limit and can allow stack overflow.

Lenient loading followed by saving a scope can remove keys and nested tables
that are not mapped by that scope. This is existing behavior, not a beta.1
change; see [saving configuration](crate::guide::files) before saving a
section that may contain keys written by another application version.

## From 0.0.7 to 0.0.8

The 0.0.8 release adds example applications; no library API or configuration
migration is required.

## From 0.0.6 to 0.0.7

The 0.0.7 release focuses on documentation; no configuration API migration is
currently required.

## From 0.0.5 to 0.0.6

Review calls to `ConfigStore::save_file` and their error handling:

- The final path component must be a missing target or a regular file. Final
  symlinks, including dangling links, other non-regular files and Windows
  reparse points are rejected with `Error::Io` and `InvalidInput`. If saving
  through a symlink was intentional, explicitly resolve its referent and pass
  that path when appropriate. Links in parent directories remain supported.
- A failed replacement with `AlreadyExists` is returned instead of deleting
  the target and retrying. Handle the error rather than relying on the old
  fallback.
- On Unix, replacing an existing target preserves its observed `rwx` mode
  bits. Do not rely on the previous creation defaults to change permissions;
  review ownership, ACLs and other metadata separately, as they are not
  preserved. New files still use the normal creation policy.
- A failed save whose temporary-file cleanup also fails returns
  `Error::SaveFileCleanup`, containing both errors. Review any error handling
  that needs to distinguish this case.

For the full file-target and access limitations, see
[Saving to files](crate::guide::files).

## From 0.0.4 to 0.0.5

The API was renamed, in-memory store operations became synchronous, and
defaults and missing nested tables changed. Update the dependency feature
`with-derive` to `derive`, then adjust calls and types as follows:

| 0.0.4 | 0.0.5 |
| --- | --- |
| `Confer` | `ConfigStore` |
| `SharedConfer` (`Arc<Confer>`) | `ConfigStore` |
| `Confer::new()` | `ConfigStore::new()` |
| `Confer::from_string(source)?` | `ConfigStore::from_str(source)?` or `source.parse::<ConfigStore>()?` |
| `Confer::from_file(path)?` (blocking) | `ConfigStore::from_file(path).await?`; outside a runtime `ConfigStore::from_str(&std::fs::read_to_string(path)?)?` |
| `Confer::from_file_async(path).await?` | `ConfigStore::from_file(path).await?` |
| `store.get_section_table(section).await` | `store.get_section(section)` |
| `store.set_section_table(section, table).await?` | `store.set_section(section, table)?` |
| `store.section_exists(section).await` | `store.has_section(section)` |
| `store.add_section(section).await?` | `store.ensure_section(section)?` |
| `store.remove_section(section).await?` | `store.remove_section(section)` — returns `()` |
| Other in-memory methods, e.g. `store.get_integer(section, key).await?` | The same name without `.await`: `store.get_integer(section, key)?` |
| `store.load_file(path).await?`, `store.save_file(path).await?` | Unchanged |
| `ConferModule` (trait and derive) | `ConfigScope` |
| `SharedConferModule<T>` | `SharedScope<T>` |
| `T::from_confer(store.clone()).await?` | `T::from_store(&store)?` |
| `T::load(&module, store.clone()).await?` | `T::load(&scope, &store).await?` |
| `T::save(&module, store.clone()).await?` | `T::save(&scope, &store).await?` |
| `ConferTable` (trait and derive) | `ConfigTable` |
| `ConferError` | `Error` |
| `ConferError::value_parse_owned(...)` | `Error::value_parse(...)` |
| Paths through `conferencier::confer_module`, `conferencier::confer_table`, `conferencier::error` | The crate root, e.g. `conferencier::ConfigScope` |
| Feature `with-derive` | Feature `derive` |
| `Error::MissingKey`, `TypeMismatch` or `ValueParse` with a `section` field | Use the `path` field; constructor arguments use the formatted path |
| A non-table section mismatch using `key == "<section>"` | An empty `path` and the raw section name in `key` |
| A nested key with U+0080–U+009F reported literally in an error path | The character is reported as `\u0080`–`\u009F` |

Manual scope implementations drop `#[async_trait]`, make `from_store`
synchronous, borrow `&ConfigStore`, and keep `async fn load` and `async fn save`.
Manual table implementations only need the new names. Existing helper
attributes keep the `#[confer(...)]` name. The store is now a cloneable handle:
its clones share the same document without an additional `Arc`.

### Section names

The default section is now the full struct name; a leading `Confer` word is no
longer stripped. A scope such as `ConferApp` without an explicit section fails
to compile during the migration period; a table-only `ConfigTable` derive has
no section and is not checked. Choose the section that matches the
existing configuration:

- Add `#[confer(section = "App")]` to keep `[App]`.
- Or add `#[confer(section = "ConferApp")]` and rename the TOML section to
  `[ConferApp]`.

This diagnostic stays through 0.1.x and is planned for removal in 0.2.0. A
direct upgrade from 0.0.4 to 0.2.0 would bypass it: add an explicit section
before upgrading, or check the section names through a 0.1.x release.

### Defaults and tables

`default = ...` now takes a Rust expression of exactly the field's type,
without implicit conversions. Defaults run only for missing keys and are saved
as ordinary values.

| 0.0.4 | 0.0.5 |
| --- | --- |
| `String` with `default = "localhost"` | `default = "localhost".into()` |
| `f32` or `f64` with `default = 1` | `default = 1.0` |
| `Vec<T>` with `default = [1, 2]`, or `default = []` | `default = vec![1, 2]`, or bare `default` |
| `Vec<String>` with `default = ["a", "b"]` | `default = vec!["a".into(), "b".into()]` |
| `Datetime` with `default = "2024-01-01T00:00:00Z"` | `default = "2024-01-01T00:00:00Z".parse().unwrap()` |
| `Vec<Datetime>` with string literals in `default` | Parse each string inside `vec![...]` |
| `Option<T>` or `Option<Vec<T>>` with a `default` | Use the inner type and an expression default to preserve the fallback; or keep `Option` and remove `default` to load a missing key as `None` |
| `#[confer(table)] server: T` relying on an empty-table load | `#[confer(table, field_defaults)]` to preserve it, or use a Rust `default` if that value is intended |
| `#[confer(table)] upstream: Vec<T>` relying on an empty vector | `#[confer(table, default)]` |
| `#[confer(table)] server: Option<T>` | Unchanged: a missing key is `None` |

Datetime defaults are parsed when needed; an invalid string in an expression
using `unwrap()` panics on a missing key instead of failing at compile time.

For a missing single table, choose the intended behavior explicitly:

| Declaration | Missing table |
| --- | --- |
| `#[confer(table)] server: T` | `MissingKey` |
| `#[confer(table, default)] server: T` | Rust `T::default()` |
| `#[confer(table, field_defaults)] server: T` | Decode an empty table, including inner defaults and required-key errors |
| `#[confer(table)] server: Option<T>` | `None` |

**Table migration warning:** A derived Rust `Default` produces `0` for a `u16`
field with `#[confer(default = 8080)]`; a missing table decoded using
`field_defaults` produces `8080`. Use `field_defaults` to preserve the old
empty-table behavior, or check the `Default` implementation and supply an
expression if a different value is intended. Migrate nested table fallbacks
recursively.

For example, this scope keeps the old `[App]` section and missing-table
fallbacks after migration:

```rust
use conferencier::{ConfigScope, ConfigStore, ConfigTable};

#[derive(ConfigTable)]
struct Server {
    #[confer(default = 8080)]
    port: u16,
}

#[derive(ConfigTable)]
struct Upstream {
    url: String,
}

#[derive(ConfigScope)]
#[confer(section = "App")]
struct ConferApp {
    #[confer(table, field_defaults)]
    server: Server,
    #[confer(table, default)]
    upstream: Vec<Upstream>,
}

# #[tokio::main]
# async fn main() -> conferencier::Result<()> {
let store = ConfigStore::from_str("[App]")?;
let app = ConferApp::from_store(&store)?;
let app = app.read().await;
assert_eq!(app.server.port, 8080);
assert!(app.upstream.is_empty());
# Ok(())
# }
```

An empty `Vec<T>` table field is now saved as `key = []` instead of being
omitted. TOML does not allow appending `[[key]]` entries after a static array;
remove or replace the array first. A struct deriving only `ConfigTable` now
rejects `#[confer(section = "...")]`: remove the attribute, or also derive
`ConfigScope` if it is intended to be a top-level scope.

## From 0.0.3 to 0.0.4

`ConferError` became non-exhaustive. Add a wildcard arm to exhaustive matches
outside the crate; constructing or matching individual variants still works.

`#[confer(init = ...)]` was removed. Ignored fields now use an optional Rust
initializer expression:

| 0.0.3 | 0.0.4 |
| --- | --- |
| `#[confer(ignore, init = "Vec::new()")]` | `#[confer(ignore = Vec::new())]`, or `#[confer(ignore)]` when the value equals `Default::default()` |
| `#[confer(ignore, default = 5)]` | `#[confer(ignore = 5)]` |
| `#[confer(ignore, rename = "...")]` | `#[confer(ignore)]`; `rename` had no effect on an ignored field |
| `#[confer(init = ...)]` on a loaded field | Remove `init`; add `#[confer(default = ...)]` only if a missing key should fall back to a value |

Expressions are written directly, not as strings, and run in a closure: they
cannot use `store`, other fields or `.await`; `return` and `?` end the
expression, not `from_confer`. Ignored fields, including those using
`Default::default()`, are initialized once after all loaded fields convert.
If conversion fails they are not initialized, so side effects may occur later
than before or not at all.

Loads and saves now work on whole sections. A failed load leaves the module
unchanged; a failed conversion on save leaves the store unchanged. In
particular, loading a module with no loaded fields now fails if its section
exists but is not a table. Review code that relied on partial updates.

## From 0.0.2 to 0.0.3

- Check scopes relying on implicitly named sections. A leading `Confer` is now
  stripped only once and only as a separate CamelCase word:
  `ConferenceSettings` uses `[ConferenceSettings]` instead of `[enceSettings]`,
  and `ConferConferApp` uses `[ConferApp]` instead of `[App]`. To retain an
  existing section, add an explicit `#[confer(section = "...")]`. This rule
  changes again in 0.0.5.
- Public TOML values, tables, datetimes and errors now come from `toml` 1.2
  and are not interchangeable with `toml` 0.9 types. Prefer the
  `conferencier::toml` and `conferencier::Datetime` re-exports, or update your
  direct TOML dependency. In the new datetime API, `Time::second` and
  `Time::nanosecond` are optional; `None` means omitted rather than zero.
- Raw identifiers no longer include the `r#` prefix in implicit keys or
  section names. For example, `r#type` now maps to `type` instead of `r#type`.
  Rename existing TOML entries, or use an explicit `rename` or `section`
  attribute to retain the old name.
- Invalid `Datetime` literal defaults are now rejected at compile time. Fix
  any invalid default rather than relying on a runtime panic.
- Rust 1.88 is required by 0.0.3. The minimum returns to 1.85 in 0.0.4.

## From 0.0.1 to 0.0.2

No breaking changes or migration steps were recorded for 0.0.2.

## 0.0.1

The initial release has no earlier version to migrate from.
