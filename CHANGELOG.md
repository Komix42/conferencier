# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.5] - 2026-11-13

### Added
- Expanded rustdoc for the complete public API, with examples, conversion and error semantics, snapshot behavior, scope lock guidance and the current file-write limitations. The READMEs explain scopes, the `derive` feature and `#[confer(...)]`, and include the updated reload examples.
- Public `table_path::{section_path, child_path, element_path}` helpers build the derive's diagnostic paths in manual `ConfigScope` and `ConfigTable` implementations, with or without the `derive` feature ([RFC-0006](docs/RFC-0006_strict_unknown_keys.md)).
- `#[confer(deny_unknown_keys)]` rejects unmapped keys during load, and `Error::UnknownKey` reports them with the formatted table path and raw key ([RFC-0006](docs/RFC-0006_strict_unknown_keys.md)).

### Changed
- `ConfigStore::save_file` uses a temporary filename independent of the target filename, so saving files with long valid names no longer exceeds the filesystem's per-component filename limit.
- **Breaking:** A struct deriving only `ConfigTable` rejects `#[confer(section = "...")]` instead of silently ignoring it. Remove the attribute, or derive `ConfigScope` as well if a top-level scope is intended ([RFC-0008](docs/RFC-0008_table_section.md)).
- **Breaking:** Missing table `T` and table `Vec<T>` fields now follow the same rule as scalar fields: use an explicit fallback, use `None` for an `Option`, or return `Error::MissingKey`. `#[confer(table, field_defaults)]` explicitly decodes a missing single table from an empty table; `#[confer(table, default)]` uses the field type's Rust `Default` instead ([RFC-0007](docs/RFC-0007_default_expressions.md)).
- **Breaking:** `#[confer(default = <expr>)]` takes a Rust expression of exactly the field's type, without implicit literal conversions; `#[confer(default)]` uses the field type's Rust `Default`. Both forms are accepted on table fields, but rejected on syntactically recognized `Option` fields. Defaults run only for missing keys and are saved as ordinary values ([RFC-0007](docs/RFC-0007_default_expressions.md)).
- **Breaking:** Empty `Vec<T>` table fields are now saved as `key = []` instead of being omitted, so they round-trip as empty even when loading requires a value. TOML does not allow adding `[[key]]` entries after a static array, so remove or replace the array first ([RFC-0007](docs/RFC-0007_default_expressions.md)).
- **Breaking:** `Error::MissingKey`, `TypeMismatch` and `ValueParse` now store the formatted table path in `path` instead of `section`. Their constructors also take a formatted path; diagnostics quote section and key segments when needed, and non-table top-level entries use an empty path with the entry name as the key. Scalar-array conversion errors identify the zero-based element index ([RFC-0006](docs/RFC-0006_strict_unknown_keys.md)).
- **Breaking:** Renamed the public API to `ConfigStore`, `ConfigScope`, `SharedScope`, `ConfigTable` and `Error`; `from_confer` is now `from_store`, the derive feature is `derive`, and the old public modules and `value_parse_owned` helper are removed. See [RFC-0005](docs/RFC-0005_api_redesign.md) for the full migration.
- **Breaking:** `ConfigStore` is a cloneable handle whose clones share one document; `SharedConfer` is gone. Its in-memory methods are synchronous (no `.await`), and only `from_file`, `load_file` and `save_file` stay async. The store uses `std::sync::RwLock` and recovers from a poisoned lock.
- **Breaking:** `from_string` is now `from_str` (`ConfigStore` also implements `FromStr`), `from_file` is async and replaces `from_file_async`, and there is no blocking file constructor. `get_section_table`, `set_section_table`, `section_exists` and `add_section` are now `get_section`, `set_section`, `has_section` and `ensure_section`; `remove_section` returns `()`.
- **Breaking:** `ConfigScope::from_store` is synchronous, and `from_store`, `load` and `save` borrow the store (`&ConfigStore`) instead of taking it by value.
- **Breaking:** `ConfigScope` no longer uses `#[async_trait]`: `load` and `save` return `impl Future<Output = Result<()>> + Send`, so generic code can spawn them. Manual implementations drop `#[async_trait]` and keep `async fn load`/`async fn save`. The `async-trait` dependency is removed.
- **Breaking:** The default section name is the full struct name; a leading `Confer` word is no longer stripped. A `ConfigScope` such as `ConferApp` without `#[confer(section = ...)]` fails to compile with a message naming both choices: `section = "App"` keeps the 0.0.4 section, `section = "ConferApp"` uses the full name. This check stays through 0.1.x and is removed in 0.2.0, so upgrade from 0.0.4 through a 0.1.x release, or add an explicit section before upgrading to 0.2.0 directly.
- **Breaking:** Quoted nested segments of diagnostic paths escape U+0080–U+009F as uppercase `\uXXXX`, like the other control characters, instead of writing them literally ([RFC-0006](docs/RFC-0006_strict_unknown_keys.md) P2).

#### Migration from 0.0.4

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
| other in-memory methods, e.g. `store.get_integer(section, key).await?` | the same name without `.await`: `store.get_integer(section, key)?` |
| `store.load_file(path).await?`, `store.save_file(path).await?` | unchanged |
| `ConferModule` (trait and derive) | `ConfigScope` |
| `SharedConferModule<T>` | `SharedScope<T>` |
| `T::from_confer(store.clone()).await?` | `T::from_store(&store)?` |
| `T::load(&module, store.clone()).await?` | `T::load(&scope, &store).await?` |
| `T::save(&module, store.clone()).await?` | `T::save(&scope, &store).await?` |
| `ConferTable` (trait and derive) | `ConfigTable` |
| `ConferError` | `Error` |
| `ConferError::value_parse_owned(...)` | `Error::value_parse(...)` |
| paths through `conferencier::confer_module`, `conferencier::confer_table`, `conferencier::error` | the crate root, e.g. `conferencier::ConfigScope` |
| feature `with-derive` | feature `derive` |
| struct `ConferApp` relying on the default section `[App]` | add `#[confer(section = "App")]` to keep it, or add `#[confer(section = "ConferApp")]` and rename the TOML section to `[ConferApp]` |
| a nested key with U+0080–U+009F reported literally in an error path | the character is reported as `\u0080`–`\u009F` |
| `Error::MissingKey`, `TypeMismatch` or `ValueParse` with a `section` field | use the `path` field; constructor arguments use the formatted path |
| a non-table section mismatch using `key == "<section>"` | an empty `path` and the raw section name in `key` |
| `String` with `default = "localhost"` | `default = "localhost".into()` |
| `f32` or `f64` with `default = 1` | `default = 1.0` |
| `Vec<T>` with `default = [1, 2]`, or `default = []` | `default = vec![1, 2]`, or bare `default` |
| `Vec<String>` with `default = ["a", "b"]` | `default = vec!["a".into(), "b".into()]` |
| `Datetime` with `default = "2024-01-01T00:00:00Z"` | `default = "2024-01-01T00:00:00Z".parse().unwrap()` |
| `Vec<Datetime>` with string literals in `default` | parse each string inside `vec![...]` |
| `Option<T>` or `Option<Vec<T>>` with a `default` | use the inner type and an expression default to preserve the fallback; or keep `Option` and remove `default` to load a missing key as `None` |
| `#[confer(table)] server: T` relying on an empty-table load | `#[confer(table, field_defaults)]` to preserve it, or use a Rust `default` if that value is intended |
| `#[confer(table)] upstream: Vec<T>` relying on an empty vector | `#[confer(table, default)]` |
| `#[confer(table)] server: Option<T>` | unchanged: a missing key is `None` |

Manual scope implementations drop `#[async_trait]`, make `from_store` synchronous, borrow `&ConfigStore`, and keep `async fn load` and `async fn save`. Manual table implementations only need the new names. Existing helper attributes keep the `#[confer(...)]` name. See [RFC-0005](docs/RFC-0005_api_redesign.md) for the full rationale.

Datetime defaults are parsed when needed; an invalid string in an expression using `unwrap()` panics on a missing key instead of failing at compile time.

For a missing single table, choose the intended behavior explicitly:

| Declaration | Missing table |
| --- | --- |
| `#[confer(table)] server: T` | `MissingKey` |
| `#[confer(table, default)] server: T` | Rust `T::default()` |
| `#[confer(table, field_defaults)] server: T` | decode an empty table, including inner defaults and required-key errors |
| `#[confer(table)] server: Option<T>` | `None` |

**Table migration warning:** A derived Rust `Default` produces `0` for a `u16` field with `#[confer(default = 8080)]`; a missing table decoded using `field_defaults` produces `8080`. Use `field_defaults` to preserve the old empty-table behavior, or check the `Default` implementation and supply an expression if a different value is intended. Migrate nested table fallbacks recursively.

### Removed
- The direct `toml` dependency of `conferencier-derive`; generated code and applications can use the `conferencier` re-exports.
- The unused `serde` dependency, so builds of `conferencier` no longer compile the `serde_derive` proc-macro.

### Fixed
- `#[confer(deny_unknown_keys)]` now accepts following attributes and trailing commas in the same `#[confer(...)]` list instead of mistaking them for a value.
- Concurrent `save_file` calls to the same path, including calls through cloned stores, use separate uniquely named temporary files instead of a shared `<filename>.tmp`, avoiding the previous temporary-file corruption. Every successful call installs one complete snapshot; which of several calls wins is unspecified. On platforms where replacement removes the existing target before retrying the rename, concurrent saves can still fail if another save removes the target first. The temporary file is removed when the write or rename fails.

## [0.0.4] - 2026-11-11

### Added
- `ConferTable`, a synchronous trait for plain nested configuration values, and `Confer::set_section_table` for atomic replacement of a complete section.
- `#[derive(ConferTable)]` and the `#[confer(table)]` field attribute map nested TOML tables to plain structs. A table field can be `T` (a sub-table such as `[App.server]`), `Option<T>` or `Vec<T>` (an array of tables such as `[[App.upstream]]`), in modules and in other tables, to any depth. An absent `T` is loaded from an empty table, so its fields' defaults apply; an absent `Vec<T>` loads as empty, unlike a scalar `Vec` without `default`; an empty `Vec<T>` is not written on save. Errors inside nested tables name the full path, for example `invalid value at App.upstream[1].weight: value out of range for u32`. One struct can derive both `ConferModule` and `ConferTable`.
- The `nested_tables` example and a "Nested tables" section in the README.
- The README explains what saving does to a hand-written configuration file: comments, key order and value notation are not preserved, and a module's `save` removes unknown keys and sub-tables from its section, including misspelled ones. Saving is recommended mainly for generating an initial configuration file.
- README sections on the scope of the crate, listing what it intentionally leaves out (formats other than TOML, layered sources, hot reload, validation beyond value types and format-preserving saves), on reloading configuration manually, and on keeping runtime state in a module through ignored fields, which later loads keep.
- API documentation for the variants and fields of `ConferError`, the `Result` alias and the `conferencier-derive` crate. The documentation of `ConferModule::load` and `ConferModule::save` now says what they change and that they lock the module, so a task must not call them while it holds a guard of that module; the README explains this as well.
- `docs/BACKLOG.md` collects known technical debt and ideas for future versions, which were previously spread over the future-work sections of the RFCs.
- A security policy (`SECURITY.md`) for reporting vulnerabilities privately through GitHub, a code of conduct, and issue and pull request templates.

### Changed
- **Breaking:** `ConferError` is now `#[non_exhaustive]`, so future releases can add error variants without a breaking change. A `match` on `ConferError` outside this crate needs a wildcard arm (`_ => ...`); constructing variants and matching individual variants work as before.
- **Breaking:** `#[confer(init = ...)]` was removed and `#[confer(ignore)]` takes an optional initializer: `#[confer(ignore = <expr>)]`. The expression is written directly in the attribute (a plain Rust expression, not a string), so compiler errors point at it. It is evaluated in a closure of its own, so it cannot use `store`, other fields or `.await`; `return` and `?` end the expression, not `from_confer`. A bare `#[confer(ignore)]` still uses `Default::default()`. Ignored fields, bare ones included, are now initialized once, in `from_confer`, after every loaded field has converted successfully, and not at all when `from_confer` fails; side effects of an initializer or of a `Default` implementation can therefore occur later than before, or not at all. Any use of `init`, and `ignore` combined with `default` or `rename`, fails to compile with a migration hint. Migrate as follows:
    - `#[confer(ignore, init = "Vec::new()")]` → `#[confer(ignore = Vec::new())]` (or plain `#[confer(ignore)]` when the value equals `Default::default()`)
    - `#[confer(ignore, default = 5)]` → `#[confer(ignore = 5)]`
    - `#[confer(ignore, rename = "...")]` → `#[confer(ignore)]` (`rename` had no effect on an ignored field)
    - `#[confer(init = ...)]` on a loaded field → remove it; add `#[confer(default = ...)]` only if a missing key should fall back to a value
- The derived `from_confer` builds the module directly from the values read from the store instead of creating a default instance and loading into it, so loaded fields no longer need to implement `Default`.
- Derived modules load and save whole sections. `from_confer` and `load` convert a single snapshot of the section, and `load` updates the module only if every field converts, so a failing `load` leaves the module unchanged (previously the fields before the failing one were already updated). `save` converts the whole module first and then replaces the section in a single write: readers never observe a half-saved module, and a conversion error such as a `u64` above `i64::MAX` leaves the store unchanged (previously earlier fields were already written). Such conversion errors are reported before a section that is not a table. `load` of a module without loaded fields now also fails when its section exists but is not a table.
- The "unsupported field type" compile error now says how to nest a struct: `#[confer(table)]` on the field and `#[derive(ConferTable)]` on the type.
- The crate description and the README introduction present conferencier as a minimal, single-purpose library.
- The minimum supported Rust version is lowered from 1.88 to 1.85, the lowest version that the 2024 edition and the `toml` 1.x dependency allow, so both crates build with older toolchains.

### Fixed
- A required `Datetime` field (plain type without `default`) now compiles and loads; a missing key fails with `MissingKey`.
- Ignored fields have no TOML key and no longer take part in duplicate key detection, so a struct that was rejected only because an ignored field was named like the key of another field now compiles.

## [0.0.3] - 2026-11-08

### Added
- The `ConferModule` trait is re-exported at the crate root next to the derive macro of the same name, so `use conferencier::ConferModule;` is enough to both derive the trait and call `from_confer`/`load`/`save`.
- Re-exported the `toml` crate (`conferencier::toml`) and its datetime type (`conferencier::Datetime`), so applications can use the TOML types from the public API without matching `toml` versions themselves.

### Changed
- The minimum supported Rust version is now 1.88 and is declared via `rust-version`, so older toolchains get a clear error instead of a compile failure.
- **Breaking:** the default section name strips a leading `Confer` only once and only as a separate CamelCase word, as documented. `ConferenceSettings` now maps to `[ConferenceSettings]` (previously `[enceSettings]`) and `ConferConferApp` to `[ConferApp]` (previously `[App]`). `ConferApp` still maps to `[App]`. If your config files rely on the old names, add `#[confer(section = "...")]`.
- Updated error derivation to `thiserror` 2 without changing public error variants, messages or source chaining.
- Updated macro crate-name resolution to `proc-macro-crate` 3, fixing derives for renamed, workspace-inherited dependencies.
- Updated procedural-macro parsing to `syn` 3.
- **Breaking:** both crates now use `toml` 1.2 (TOML 1.1). Public TOML values, tables, datetimes and errors are no longer interchangeable with types from `toml` 0.9. Prefer the `conferencier::toml` and `conferencier::Datetime` re-exports, or update your direct TOML dependency.
- In the new datetime API, `Time::second` and `Time::nanosecond` are optional, preserving omitted components instead of replacing them with zero.

### Fixed
- Both crates can be published together with `cargo publish --workspace`; the versioned dev-dependency of `conferencier-derive` on `conferencier` that formed a publish cycle is now path-only.
- The published `conferencier` package no longer ships a stray `target/advanced_usage_snapshot.toml` generated by the advanced example.
- Both published crates now include the `LICENSE-MIT` and `LICENSE-APACHE` texts required by their licenses.
- The README quick start now compiles and runs (it lacked the trait import, a default for `features` and the `tokio` dependency). The crate README is now the crate-level documentation and runs as a doctest, so it stays correct.
- The `conferencier-derive` README no longer suggests that the derive works without depending on `conferencier`.
- Invalid `Datetime` defaults such as `#[confer(default = "tomorrow")]` are rejected at compile time with a clear error instead of panicking at runtime.
- Code generated for `Datetime` defaults no longer requires `toml` as a direct dependency of the user's crate.
- Fields named like locals of the generated `save()` (for example `store` or `module`) no longer break compilation of the derived code.
- Fields declared with raw identifiers (for example `r#type`) map to the TOML key `type` instead of `r#type`.
- Negative numeric defaults such as `#[confer(default = -1)]` or `[-1, 2]` are accepted for signed integer and float fields; on unsigned fields they produce a clear compile error.

## [0.0.2] - 2025-09-30

### Added
- `ConferError::value_parse_owned` constructor accepting an owned `String`.
- crates.io keywords and categories for both crates.

### Fixed
- Derived modules with `f32` fields failed to compile because the generated range checks used an ambiguous `.into()`.

## [0.0.1] - 2025-09-29

### Added
- Core `conferencier` crate providing an async, TOML-backed configuration store with typed getters and setters.
- `#[derive(ConferModule)]` procedural macro for generating load/save implementations.
- Shared `Confer` store API supporting synchronous and asynchronous file I/O, atomic writes, and section/key management.
- Comprehensive error handling via `ConferError` with rich diagnostics for type mismatches and parsing issues.
- Integration and unit tests covering store operations, module round-trips, and concurrency scenarios.
- Examples demonstrating advanced usage patterns, including file-backed configuration updates.
- Developer documentation (`docs/`) describing architecture decisions and implementation details.
