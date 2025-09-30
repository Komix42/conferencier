# RFC-0001: conferencier and conferencier-derive — initial state

Date: 2025-09-20
Status: Implemented
Author: Komix42 <komix[at]ignore.cz>

## Document Scope

This document outlines the initial architecture, high‑level API, and behavior of the `conferencier` (core) and `conferencier-derive` (procedural macro) crates. It explains their responsibilities, how they interact, the error model, and includes examples of typical usage.

The repository currently contains only scaffolding code in `src/` and illustrative snippets in `docs/`; all functionality described here is pending implementation according to the roadmap below.

## Synopsis

conferencier is a lightweight, asynchronous, thread‑safe configuration and runtime‑state hub for Rust applications. It centers on a shared in‑memory TOML store and an ergonomic derive macro for mapping module structs to configuration sections. At a glance:
- Async, thread‑safe access to a shared TOML table with many readers and a single writer.
- Simple persistence: load/save from/to TOML strings and files.
- Per‑module sections: structs derived with `#[derive(ConferModule)]` read and write their own TOML section.
- Field controls: defaults, optional values, key renaming, and runtime‑only (ignored) fields.
- Typed getters/setters in the core for common scalar and vector types, including TOML `Datetime`.

Note: In API identifiers (types, functions, macros, etc.), the crate name is shortened to `confer` for ease of use.

## Table of Contents

- [Document Scope](#document-scope)
- [Synopsis](#synopsis)
- [Glossary](#glossary)
- [API surface summary](#api-surface-summary)
  - [Canonical type aliases and trait surface](#canonical-type-aliases-and-trait-surface)
  - [Core crate (`conferencier`)](#core-crate-conferencier)
  - [Derive crate (`conferencier-derive`)](#derive-crate-conferencier-derive)
- [Motivation and goals](#motivation-and-goals)
- [Scope](#scope)
- [Out of scope](#out-of-scope)
- [Roadmap / implementation milestones](#roadmap--implementation-milestones)
- [Architecture](#architecture)
  - [`conferencier` (core)](#conferencier-core)
  - [`conferencier-derive` (procedural macro)](#conferencier-derive-procedural-macro)
- [Error model reference](#error-model-reference)
- [Examples](#examples)
  - [Basic work with `Confer`](#basic-work-with-confer)
  - [Module with section, rename, and defaults](#module-with-section-rename-and-defaults)
  - [Ignored fields and init expression](#ignored-fields-and-init-expression)
  - [Option field with default and rename](#option-field-with-default-and-rename)
  - [Vec<Datetime> fallback from strings](#vecdatetime-fallback-from-strings)
- [Safety, concurrency, and performance](#safety-concurrency-and-performance)
- [Edge cases and behavior](#edge-cases-and-behavior)
- [Dependencies and compatibility](#dependencies-and-compatibility)
- [Limitations](#limitations)
- [Design trade-offs](#design-trade-offs)
- [Open questions / next steps](#open-questions--next-steps)
- [Migration notes](#migration-notes)
- [Testing strategy](#testing-strategy)
## Glossary

- Confer — The core type exposing a shared, in‑memory TOML table with async typed getters/setters and load/save. Internally wraps `tokio::sync::RwLock<toml::Table>`.
- SharedConfer — Alias `type SharedConfer = Arc<Confer>` used to cheaply share the store across tasks.
- ConferModule — Trait implemented by `#[derive(ConferModule)]` on a struct to bind it to a TOML section and generate `from_confer`, `load`, and `save`. `SharedConferModule<T> = Arc<RwLock<T>>`.
- Section / Key — Section is a top‑level TOML table “owned” by a module (default: type name without the `Confer` prefix, or overridden by `#[confer(section = "...")]`). Key is the per‑field name (overridable via `#[confer(rename = "...")]`).
- default — `#[confer(default = ...)]` supplies a compile-time-checked Rust literal that must match the field type:
  - Strings and datetimes use Rust string literals: `"host"`, `"2024-01-01T00:00:00Z"` (the datetime string is validated via `Datetime::from_str`).
  - Numbers/booleans use Rust literals: `42`, `true`.
  - Vectors use bracket syntax: `[1, 2, 3]`, `["a", "b"]`, `[true, false]`, `["2024-01-01T00:00:00Z"]`.
  The value is wrapped in `Some(...)` for `Option<T>` fields.
- init — `#[confer(init = "<expr>")]` initializes the field before loading; cannot be combined with `default`.
- rename — `#[confer(rename = "...")]` changes the stored key name within the section.
- ignore — `#[confer(ignore)]` marks a field as runtime‑only (not loaded/saved); `init/default` still apply.
- Datetime fallback — For `Datetime` and `Vec<Datetime>`, when a present value has an incompatible TOML type, loading will attempt to parse from string(s). Missing keys are not affected by this fallback.
- Numeric normalization — Integers/floats are stored as `i64`/`f64`; conversions back to field types happen on load. Out‑of‑range values yield `ValueParse` errors.
- "confer" shorthand — In code examples and API names, the crate prefix is shortened to `confer` for readability.

## API surface summary

### Canonical type aliases and trait surface

To keep naming consistent across both crates the following aliases and trait shape are treated as part of the public contract:

```rust
pub type SharedConfer = std::sync::Arc<Confer>;
pub type SharedConferModule<T> = std::sync::Arc<tokio::sync::RwLock<T>>;
pub type Result<T> = std::result::Result<T, ConferError>;

pub mod confer_module {
    use super::*;

    #[async_trait::async_trait]
    pub trait ConferModule: Send + Sync + Sized + 'static {
        async fn from_confer(store: SharedConfer) -> Result<SharedConferModule<Self>>;
        async fn load(module: &SharedConferModule<Self>, store: SharedConfer) -> Result<()>;
        async fn save(module: &SharedConferModule<Self>, store: SharedConfer) -> Result<()>;
    }
}
```

The derive crate (`conferencier-derive`) generates an implementation that adheres to the trait above, including the `Send + Sync` bounds so modules can be shared across async tasks without additional wrappers.

Re-export note: The core crate can re-export the derive macro under the `with-derive` feature (enabled by default) so projects may only depend on `conferencier`.

### Core crate (`conferencier`)

- Constructors
  - `Confer::new() -> SharedConfer`
  - `Confer::from_string(&str) -> Result<SharedConfer>`
  - `Confer::from_file(path: impl AsRef<std::path::Path>) -> Result<SharedConfer>` (blocking)
  - `Confer::from_file_async(path: impl AsRef<std::path::Path>) -> Result<SharedConfer>` (async)
- Persistence
  - `async fn load_str(&self, toml: &str) -> Result<()>` (replaces the in-memory table; destructive load)
  - `async fn load_file(&self, path: impl AsRef<std::path::Path> + Send + Sync) -> Result<()>`
  - `async fn save_str(&self) -> Result<String>`
  - `async fn save_file(&self, path: impl AsRef<std::path::Path> + Send + Sync) -> Result<()>`
- Raw accessors
  - `async fn get_value(&self, section, key) -> Option<toml::Value>`
  - `async fn get_section_table(&self, section) -> Option<toml::Table>`
  - `async fn set_value(&self, section, key, toml::Value) -> Result<()>`
  - `async fn section_exists(&self, section) -> bool`
  - `async fn add_section(&self, section) -> Result<()>`
  - `async fn remove_key(&self, section, key) -> Result<()>`
  - `async fn remove_section(&self, section) -> Result<()>`
  - `async fn list_sections(&self) -> Vec<String>`
  - `async fn list_keys(&self, section) -> Result<Vec<String>>`
  
Behavioral contract (core operations)
- All mutating section/key operations are idempotent:
  - `add_section` returns `Ok(())` even if the section already exists.
  - `remove_key` returns `Ok(())` even if the key does not exist.
  - `remove_section` returns `Ok(())` even if the section does not exist.
- `set_*` and `set_value` implicitly create the target section when it does not exist.
- `list_keys(section)` returns `Ok(vec![])` when the section does not exist.
- `get_section_table` returns a cloned snapshot of the section table; mutating the returned table has no effect on the store.
- Typed getters (scalars)
  - `async fn get_string(&self, section, key) -> Result<String>`
  - `async fn get_integer(&self, section, key) -> Result<i64>`
  - `async fn get_float(&self, section, key) -> Result<f64>`
  - `async fn get_boolean(&self, section, key) -> Result<bool>`
  - `async fn get_datetime(&self, section, key) -> Result<toml::value::Datetime>`
- Typed getters (arrays)
  - `async fn get_string_vec(&self, section, key) -> Result<Vec<String>>`
  - `async fn get_integer_vec(&self, section, key) -> Result<Vec<i64>>`
  - `async fn get_float_vec(&self, section, key) -> Result<Vec<f64>>`
  - `async fn get_boolean_vec(&self, section, key) -> Result<Vec<bool>>`
  - `async fn get_datetime_vec(&self, section, key) -> Result<Vec<toml::value::Datetime>>`
- Typed setters (scalars)
  - `async fn set_string(&self, section, key, value: String) -> Result<()>`
  - `async fn set_integer(&self, section, key, value: i64) -> Result<()>`
  - `async fn set_float(&self, section, key, value: f64) -> Result<()>`
  - `async fn set_boolean(&self, section, key, value: bool) -> Result<()>`
  - `async fn set_datetime(&self, section, key, value: toml::value::Datetime) -> Result<()>`
- Typed setters (arrays)
  - `async fn set_string_vec(&self, section, key, value: Vec<String>) -> Result<()>`
  - `async fn set_integer_vec(&self, section, key, value: Vec<i64>) -> Result<()>`
  - `async fn set_float_vec(&self, section, key, value: Vec<f64>) -> Result<()>`
  - `async fn set_boolean_vec(&self, section, key, value: Vec<bool>) -> Result<()>`
  - `async fn set_datetime_vec(&self, section, key, value: Vec<toml::value::Datetime>) -> Result<()>`
- Concurrency and sharing
  - `SharedConfer = Arc<Confer>` for cheap sharing; internal sync via `tokio::sync::RwLock`
- Option handling notes
  - Core returns `Result<T>`; derive layer maps to `Option`/defaults

Feature: with-derive (default)
- The core crate re-exports the derive macro when the `with-derive` feature is enabled (default-on). This allows users to depend on a single crate:
  - `use conferencier::Confer;`
  - `use conferencier::confer_module::{ConferModule, SharedConferModule};`
  - `#[derive(conferencier::ConferModule)] // when using the re-export`
- To opt out (e.g., to reduce compile times or avoid proc-macro in certain builds), disable default features and opt-in explicitly:
  - In Cargo.toml: `conferencier = { version = "0.0.1", default-features = false, features = ["with-derive"] }` to re-enable
  - Or without re-export: `conferencier = { version = "0.0.1", default-features = false }` and `conferencier-derive = "0.0.1"`

Examples (Cargo.toml)

```toml
# Default (derive re-export enabled via default features)
[dependencies]
conferencier = "0.0.1"

# Opt-out of re-export (no proc-macro transitively)
[dependencies]
conferencier = { version = "0.0.1", default-features = false }

# Two-crate setup (explicit derive and core)
[dependencies]
conferencier = { version = "0.0.1", default-features = false }
conferencier-derive = "0.0.1"

# Opt-in re-export without all defaults
[dependencies]
conferencier = { version = "0.0.1", default-features = false, features = ["with-derive"] }
```

### Derive crate (`conferencier-derive`)

- Trait methods generated by `#[derive(ConferModule)]`
  - `async fn from_confer(SharedConfer) -> Result<SharedConferModule<Self>>`
  - `async fn load(&SharedConferModule<Self>, SharedConfer) -> Result<()>`
  - `async fn save(&SharedConferModule<Self>, SharedConfer) -> Result<()>`
- Struct-level attributes
  - `section = "..."`
- Field-level attributes
  - `rename = "..."`
  - `default = "..."`
  - `init = "<expr>"`
  - `ignore`
- Supported field shapes
  - Scalars: `String`, integers, `f32/f64`, `bool`, `toml::value::Datetime`
  - `Vec<T>` for supported scalars
  - `Option<T>` and `Option<Vec<T>>`
  - Other/custom types: out-of-scope in v0.0.1 — use `#[confer(ignore)]` for runtime-only fields or refactor into supported scalar/vector/Option shapes.
- Semantics highlights
  - Missing required → `MissingKey`
  - `Option` missing → `None` / `Some(default)`
  - `Option` save semantics → `Some(_)` writes/overwrites the key; `None` removes the key from the section during `save`
  - `Datetime` fallback from strings, numeric normalisation via `i64`/`f64`
  - `#[confer(ignore)]` fields stay runtime-only

#### Contract of `from_confer`

`from_confer(store)` is the canonical entry point for constructing a module instance that is backed by the shared configuration:

1. The generated implementation evaluates field defaults and `init` expressions to obtain the pre-load state. Fields without explicit metadata start from their Rust default literal (e.g., `Default::default()` for standard types).
2. It immediately calls `Self::load` with the freshly constructed `SharedConferModule<Self>` so that persisted values overlay the pre-load state. All attribute semantics (`default`, `init`, `ignore`, fallbacks) apply exactly as described for `load`.
3. Any error returned by `load` (missing required key, type mismatch, etc.) aborts the construction; the shared module is not returned and the error is propagated unchanged.
4. On success the method returns an `Arc<RwLock<Self>>` that is already hydrated and ready for concurrent use. `from_confer` itself never persists data; callers must invoke `save` to write changes back.

Because `from_confer` hydrates the module through `load`, consumers can rely on the returned handle reflecting the current TOML store, while deferring writes until an explicit `save` call.

## Motivation and goals

- A shared runtime context via `SharedConfer = Arc<Confer>` (internally `tokio::sync::RwLock<toml::Table>`) that module structs derived with `#[derive(ConferModule)]` attach to.
- Ergonomic, type‑safe access through the generated asynchronous API of modules (`from_confer`, `load`, `save`).
- Unified loading/saving to TOML (file/string), including typed getters and setters in the core.
- Ability to keep runtime‑only state in modules via `#[confer(ignore)]`, which is never persisted and does not collide with TOML sections.
## Scope

- Crate `conferencier` (version 0.0.1): runtime store + typed API and errors.
- Crate `conferencier-derive` (version 0.0.1): `#[derive(ConferModule)]` and `#[confer(...)]` attributes for mapping fields to TOML and defining load/save behavior.

## Out of scope

The initial release intentionally leaves the following capabilities unimplemented; they may be revisited in later RFCs:

- **Hot reload / watchers** — no background file monitoring or automatic propagation of external changes.
- **Schema validation hooks** — aside from basic type checking, there is no built-in range/regex validation or custom attribute-driven validators.
- **Nested table projections** — only flat per-section tables are supported; hierarchical structures require manual decomposition.
- **Pluggable serialization formats** — TOML is the only storage backend; JSON/YAML or database persistence is not addressed.
- **User-defined conversion hooks** — conversion traits such as `#[confer(with = ...)]` are deferred to future work.

## Roadmap / implementation milestones

1. **Core storage skeleton** — implement `Confer` constructors, raw accessors, and persistence with comprehensive unit tests.
2. **Typed API layer** — add typed getters/setters plus error variants; exercise numeric and datetime edge cases.
3. **Procedural macro MVP** — derive `ConferModule` with section naming, `rename`, `default`, `init`, and `ignore` attributes; cover both happy paths and compile-time validation via `trybuild`.
4. **Module integration tests** — create sample modules representing realistic configuration slices, verifying `load/save` roundtrips.
5. **Documentation & examples** — polish README, expand examples in this RFC, and prepare migration guidance ahead of publishing v0.1.0.

## Architecture
 
Note on naming: In code examples and API references below, the crate is referred to using the shorter `confer` prefix for convenience.

### `conferencier` (core)

The public API enumerated in [API surface summary](#api-surface-summary) is implemented around the following design pillars:

- Type `Confer`: an in-memory `toml::Table` guarded by `tokio::sync::RwLock<Table>`.
- Shared access through `SharedConfer` (see [canonical aliases](#canonical-type-aliases-and-trait-surface)).
- Per-section addressing: every operation works with a `(section, key)` tuple and never mutates unrelated data.
- Serialization/deserialization flows reuse Tokio's filesystem primitives for async calls and pure `toml` parsing/serialization for in-memory variants (via `load_str` / `save_str`).

#### Asynchronous execution model

All public methods on `Confer` other than the constructors are asynchronous for two reasons:

1. **Lock acquisition** — the internal `RwLock` is asynchronous, so both readers and writers integrate with Tokio's scheduler without blocking threads.
2. **Uniform ergonomics** — even memory-only operations share the same `async` shape, which avoids having to juggle sync/async variants in the derive-generated code and keeps call sites consistent.

Blocking I/O (`from_file`) is intentionally limited to the constructor for use cases that initialise configuration before entering an async runtime. All subsequent persistence flows favour async I/O to prevent executor stalls.

Atomicity and snapshot guarantees
- `save_file` writes to a temporary file and then performs an atomic rename where supported by the platform, minimising the risk of partial writes. The in-memory table is serialized under a read lock to provide a consistent snapshot.
- `load_str`/`load_file` replace the in-memory table under a write lock atomically.

#### Interaction with derived modules

The derive macro translates field-level metadata into calls to the typed getters/setters. Option semantics, defaults, and runtime-only fields are enforced in the generated code so the core remains agnostic of higher-level module rules.

Modules treat their TOML section as an authoritative projection of their fields and they fully own that section. On `save`, the implementation overwrites keys corresponding to each non-ignored field, removes keys whose value is `None`, and removes any unknown keys present in the module's own section that are not mapped by the module's declared non-ignored fields. Keys in other sections (owned by other modules or manual edits outside this module's section) are always left untouched.

Save semantics
- `save` persists the module's current in-memory state, including values originating from `default` and/or `init`. This ensures the store becomes a canonical projection of the module state after a save.
- Warning: because unknown keys within the module's section are removed on `save`, do not store ad‑hoc keys in a module‑owned section; instead, place them in a separate section.

Notes about `Option` handling (derive-level semantics)

- The core getters return `Result<T>`; the derive layer wraps them into `Option<T>` or defaulted values when attributes request it.
- Missing required keys bubble up as `ConferError::MissingKey` to aid early detection.
- `#[confer(ignore)]` fields never touch the store, ensuring runtime-only state does not collide with persisted configuration.

## Error model reference

The public `ConferError` enum is expected to expose the following variants:
- `Io { path: Option<PathBuf>, source: std::io::Error }`
- `Parse(toml::de::Error)`
- `Serialize(toml::ser::Error)`
- `MissingKey { section: String, key: String }`
- `TypeMismatch { section: String, key: String, expected: &'static str, found: &'static str }`
- `ValueParse { section: String, key: String, message: String }`

### Store-level operations

| Operation family | Failure variants | Notes |
| --- | --- | --- |
| Constructors: `from_string`, `from_file`, `from_file_async` | `ConferError::Parse`, `ConferError::Io` | `Parse` wraps TOML syntax errors; `Io` bubbles up filesystem failures with the offending path. |
| Persistence: `load_str`, `load_file` | `ConferError::Parse`, `ConferError::Io` | Replace the in-memory table; `load_str` never touches the filesystem. |
| Persistence: `save_str`, `save_file` | `ConferError::Serialize`, `ConferError::Io` | Serializing to string uses `toml::ser`; IO failure only arises for `save_file`. |
| Raw getters: `get_value`, `get_section_table` | `Ok(None)` when missing | Absence is modelled by `Option`; no error variant is raised. |
| Typed getters (`get_*`, `get_*_vec`) | `ConferError::MissingKey`, `ConferError::TypeMismatch`, `ConferError::ValueParse` | `MissingKey` only when the section/key pair does not exist; `TypeMismatch` when TOML type differs; `ValueParse` when conversion (e.g., numeric range) fails. |
| Typed setters (`set_*`, `set_*_vec`) | — | Setters accept already-typed values; conversion/validation happens before core calls (e.g., in the derive layer). |
| Derived API: `ConferModule::{from_confer, load, save}` | Propagates the variants above | Runtime errors typically originate in core operations; the derive layer may also emit `ValueParse` during conversions it performs before invoking core (e.g., integer narrowing). Attribute validation errors are compile-time (`compile_error!`). |

### Type normalisation and fallbacks

| Field type | Accepted TOML inputs | Fallback behaviour | Failure variant |
| --- | --- | --- | --- |
| `String`, `Vec<String>` | `String`, `Array<String>` | None | `TypeMismatch` when encountering non-string values |
| Integers (`i*`, `u*`), `Vec<i64>` | `Integer`, `Array<Integer>` | Signed values normalised to `i64`; unsigned enforce non-negativity | `ValueParse` when out of range/negative for unsigned |
| Floats (`f32/f64`), `Vec<f64>` | `Float`, `Integer`, arrays thereof | Integers auto-upcast to float | `TypeMismatch` if array elements are not numeric |
| `bool`, `Vec<bool>` | `Boolean`, `Array<Boolean>` | None | `TypeMismatch` for non-boolean inputs |
| `Datetime`, `Vec<Datetime>` | `Datetime`, `Array<Datetime>` | When encountering strings (scalar or array of strings), tries `Datetime::from_str` | `ValueParse` if parsing fails after fallback |
| `Option<T>` / `Option<Vec<T>>` | Same as `T` | Missing key → `Ok(None)`; `default` attribute wraps literal to `Some(...)` | Propagates errors from `T` when the key is present |
| Other/custom user types | Out-of-scope in v0.0.1 | N/A | N/A |

### `conferencier-derive` (procedural macro)

- Derive `#[derive(ConferModule)]` implements `confer::confer_module::ConferModule` and generates an async API on the type:
  - `async fn from_confer(SharedConfer) -> Result<SharedConferModule<Self>>`
  - `async fn load(&SharedConferModule<Self>, SharedConfer) -> Result<()>`
  - `async fn save(&SharedConferModule<Self>, SharedConfer) -> Result<()>`
- Module alias: `type SharedConferModule<T> = Arc<RwLock<T>>`.
- Section: by default the type name with a leading `Confer` prefix stripped (e.g., `ConferApp` → `App`); can be overridden via `#[confer(section = "Srv")]`.
  - If the type name does not start with `Confer`, the full type name is used unchanged as the default section name.
- Generated loading uses the `Confer` API and behaves according to field types and attributes (see below).
 - Key collisions within a module: if two non-ignored fields resolve to the same key (after `rename` or default naming), the macro emits a compile-time error.

Supported field types
- Scalars: `String`, integers (`i8..i64`, `u8..u64` with the runtime value ≤ `i64::MAX` due to TOML's 64‑bit signed integer), `f32`/`f64`, `bool`, `toml::value::Datetime` (`Datetime`).
- `Vec<T>` for the above types.
- `Option<T>` and `Option<Vec<T>>` for the above types.
- Other/complex user-defined types are out-of-scope in v0.0.1. Use `#[confer(ignore)]` to keep such fields runtime-only, or decompose them into supported scalar/vector shapes.

Attributes `#[confer(...)]`
- `default = <literal or [ ... ]>`
  - Scalars: Rust literals (`"host"`, `42`, `true`).
  - Datetime: a Rust string containing a TOML datetime (`"2024-01-01T00:00:00Z"`), validated via `Datetime::from_str` at compile time.
  - Vectors: bracket syntax with elements matching the field type, e.g., `[1, 2, 3]`, `[true, false]`, `["a", "b"]`, `["2024-01-01T00:00:00Z"]`.
  - For `Option<T>`, the default is wrapped in `Some(...)`.
  - The derive performs compile-time validation of tokens and types; mismatches cause `compile_error!`.
- `init = "<rust‑expr>"`
  - Any Rust `Expr` used to initialize the field before loading (e.g., `Vec::new()`, `Some(5)`, more complex literals/tuples).
-  - Cannot be combined with `default` on the same field — compilation fails.
- `ignore`
-  - The field is neither loaded nor saved; `init`/`default` still apply (if present). Useful for runtime‑only state.
- `section = "<name>"` (struct‑level) — override section name in TOML.
- `rename = "<key>"` (field‑level) — override key name within the section.

Loading (load)
- Required fields (no `default`, not `Option<...>`):
  - Missing key → `MissingKey`.
- Fields with `default` (or `Option<...>`):
  - If a value is present and type‑matches → it is set; otherwise the default/`None` remains.
- `Datetime` and `Vec<Datetime>` have fallback: if typed extraction fails, parsing from string/vector of strings is attempted. This fallback applies only when a value is present but of an incompatible TOML type; it does not apply to missing keys.
  - The fallback is implemented by the core typed getters (`get_datetime`, `get_datetime_vec`).
- `#[confer(ignore)]` — the field is not loaded at all (stays `Default`/`init`/`Some(default)` depending on combination).

Saving (save)
- Values are saved into the module’s TOML section under keys per names/`rename`.
- Before writing field values, the module's section is reconciled so that any keys not corresponding to the module's declared non-ignored fields are removed. Other sections are not modified.
- Ignored fields (`#[confer(ignore)]`) are never persisted.
- `Option<T>` and `Option<Vec<T>>`: saved only when `Some(...)`.
- Numeric types are saved as `i64`/`f64` (conversion on write); `Datetime` as TOML datetime.
- Other/complex user-defined types are out-of-scope in v0.0.1. Prefer `#[confer(ignore)]` for runtime-only state or handle them manually outside the derive flow.

Sections, renaming, collisions
- Each module strictly owns its TOML section (type name without the `Confer` prefix, or overridden via `section = ...`). Unknown keys within that section are removed during `save` to keep the section a canonical projection of the module's declared non-ignored fields. Other sections remain untouched.
- `rename` changes the key name within the section.
- Ignored runtime fields do not enter the section, so there are no collisions between runtime state and persisted configuration.
 - A single section must not be shared by multiple modules in the same process; such configuration is unsupported and may lead to conflicting saves.

## Examples

### Basic work with `Confer`

```rust
use conferencier::{Confer, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let c = Confer::from_string("[App]\nname=\"demo\"\n")?;
    assert_eq!(c.get_string("App", "name").await?, "demo");

  c.set_integer("App", "port", 8080).await?;
  c.set_string_vec("App", "langs", vec!["en".into(), "de".into()]).await?;

    // c.save_file("config.toml").await?;
    Ok(())
}
```

### Module with section, rename, and defaults

```rust
use conferencier::{Confer, Result};
use conferencier::confer_module::{ConferModule, SharedConferModule};
use conferencier_derive::ConferModule;

#[derive(ConferModule)]
#[confer(section = "Srv")] // section [Srv]
struct Server {
    #[confer(rename = "p")] // key "p"
    port: u16,
    #[confer(default = "0.0.0.0")]
    host: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Confer::from_string("[Srv]\np=8080\n")?;
    let srv: SharedConferModule<Server> = Server::from_confer(cfg.clone()).await?;
    Server::save(&srv, cfg.clone()).await?; // persist current state back to the TOML store
    Ok(())
}
```

### Ignored fields and init expression

```rust
use conferencier_derive::ConferModule;
use toml::value::Datetime;

#[derive(ConferModule)]
struct Example {
    // runtime-only; not loaded or saved, but init applies
    #[confer(ignore, init = "Vec::new()")]
    cache: Vec<u8>,

    // required field with fallback parsing from string
    build_time: Datetime,
}
```

### Option field with default and rename

```rust
use conferencier_derive::ConferModule;

#[derive(ConferModule)]
#[confer(section = "App")] // section [App]
struct Settings {
  #[confer(rename = "h", default = "localhost")] // Some("localhost") when missing
  host: Option<String>,
}

// TOML
// [App]
// h = "example.com"  # when present → host = Some("example.com")
// # when absent      → host = Some("localhost")
```

### Vec<Datetime> fallback from strings

```rust
use conferencier_derive::ConferModule;
use toml::value::Datetime;

#[derive(ConferModule)]
#[confer(section = "Build")] // section [Build]
struct BuildInfo {
  // Accepts TOML datetime array or array of strings parsable as datetime
  times: Vec<Datetime>,
}

// TOML (both forms are accepted)
// [Build]
// times = [ 2024-01-01T00:00:00Z, 2024-06-01T12:34:56Z ]
// # or
// times = [ "2024-01-01T00:00:00Z", "2024-06-01T12:34:56Z" ]
```

### Load, mutate, and save lifecycle

```rust
use conferencier::{confer_module::ConferModule, Confer, Result};
use conferencier_derive::ConferModule;

#[derive(ConferModule)]
#[confer(section = "Flags")]
struct FeatureToggle {
  enabled: bool,
  #[confer(default = 3)]
  retries: u8,
  note: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
  let cfg = Confer::from_string("[Flags]\nenabled = true\nnote = \"temp\"\n")?;
  let module = FeatureToggle::from_confer(cfg.clone()).await?;

  {
    let mut guard = module.write().await;
    guard.enabled = false;
    guard.note = None; // removing the note should clear the key on save
  }

  FeatureToggle::save(&module, cfg.clone()).await?;

  assert!(!cfg.get_boolean("Flags", "enabled").await?);
  assert!(cfg.get_value("Flags", "note").await.is_none());
  Ok(())
}
```

### Unknown keys in a module's section are removed on save

```rust
use conferencier::{confer_module::ConferModule, Confer, Result};
use conferencier_derive::ConferModule;

#[derive(ConferModule)]
#[confer(section = "Srv")] // section [Srv]
struct Server {
  #[confer(rename = "p")]
  port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
  // Note the unknown key `extra` inside the [Srv] section
  let cfg = Confer::from_string("[Srv]\np = 8080\nextra = 123\n[Other]\nkeep = true\n")?;

  let module = Server::from_confer(cfg.clone()).await?;

  // Saving should reconcile [Srv]: keep known keys (p), remove unknown (extra),
  // and leave other sections ([Other]) untouched.
  Server::save(&module, cfg.clone()).await?;

  assert!(cfg.get_value("Srv", "p").await.is_some());
  assert!(cfg.get_value("Srv", "extra").await.is_none());
  assert!(cfg.get_value("Other", "keep").await.is_some());
  Ok(())
}
```

## Safety, concurrency, and performance

- `Confer` uses `tokio::sync::RwLock<Table>`: many concurrent readers, single writer.
- Most public methods are asynchronous; constructors include both synchronous and asynchronous variants. File operations use Tokio async I/O.
- Generated code does not use `unwrap/expect` on untrusted data paths; errors are surfaced via `ConferError`.

## Edge cases and behavior

- Missing required key (no `default`, not `Option`) → `MissingKey`.
- Type mismatch on a required field → `TypeMismatch` or `ValueParse` (e.g., `Datetime` from string).
- `Option<T>`/`Option<Vec<T>>`: when the key is missing it remains `None` (unless `default` or `init` dictates otherwise); during `save`, `None` clears the stored key while `Some(value)` writes/overwrites it.
- `Datetime`/`Vec<Datetime>`: if typed read fails, string(s) are attempted; for required fields an error occurs only after parse failure.
- `#[confer(ignore)]` can be used for unsupported types (e.g., tuple); such fields remain at `Default`/`init` and are skipped on save.

## Dependencies and compatibility

- `conferencier`: `tokio = 1` (features `sync`, `macros`, `rt`, `rt-multi-thread`, `fs`), `toml = 0.9`, `thiserror = 1.0`, `serde = 1.0` (derive), `tempfile` (tests).
  - Feature `with-derive` (default): re-exports the derive macro so downstream users can write `#[derive(ConferModule)]` without depending on `conferencier-derive` directly. Implemented as an optional dependency on `conferencier-derive`.
  - To opt out of the re-export: `conferencier = { version = "0.0.1", default-features = false }` and optionally add `conferencier-derive = "0.0.1"` explicitly.
- `conferencier-derive`: `syn` (full), `quote`, `proc-macro2`, `proc-macro-crate`, `toml = 0.9`; no runtime dependency on `conferencier`. The macro generates an impl of the trait `::conferencier::confer_module::ConferModule` and uses `proc-macro-crate` to resolve the core crate path.

## Limitations

- Support for arbitrary/custom user types (beyond documented scalars, vectors, and Options) is out-of-scope in v0.0.1. Use `#[confer(ignore)]` for runtime-only fields or decompose into supported shapes.
- No per‑struct pluggable configuration (e.g., hooks before/after load/save) — could be extended in the future.
- Numeric types are normalized via `i64`/`f64` on load/save. Out-of-range values yield `ValueParse` errors; no silent truncation.

Formatting and comments
- Serialization via `toml::ser` does not preserve original file formatting, key order, or comments; `save_file` writes a fresh serialized representation.

Numeric normalization details
- TOML integers are 64‑bit signed; the store normalizes integers to `i64`. As a consequence, only integer values within `i64` range are round‑trip safe. For unsigned targets, non‑negativity is enforced.
- On save, integers and floats are normalized to `i64`/`f64` in the core store. The derive layer may fail `save` with `ValueParse` if a field value cannot be represented as `i64` (e.g., `u64` > `i64::MAX`).
- On load, conversions to the target field type are performed by the derive‑generated code. If a value is out of range for the target type (e.g., negative → unsigned, or exceeding bit‑width), loading fails with `ValueParse` for that field rather than silently truncating.
- For `f32` targets, non-finite or out-of-range values during conversion from `f64` result in `ValueParse`.

## Open questions / next steps

- Extend attributes with user‑defined conversion hooks (`#[confer(with = ...)]`).
- Optional schema validation (e.g., ranges, regexes) at the derive level.
- Broaden supported data types (maps, nested structures) beyond `Vec<T>` and scalars.
- Tooling for hot‑reloading configuration from external sources and propagating into modules.

## Migration notes

- If migrating from a static TOML loader to `confer/ConferModule`, start by defining module structs with `#[derive(ConferModule)]` and move keys into the corresponding section.
- Use `default` for compatibility with missing keys, `rename` to align with existing TOML names, and `ignore` for runtime state.
- For time values use `toml::value::Datetime`; fallback parsing from string works on read/write.

## Design trade-offs

- TOML as the storage format: human-friendly, widely supported, and maps cleanly to the types we expose; trade‑off is limited native support for extremely large or deeply nested structures.
- `RwLock<Table>`: many concurrent readers and a single writer fit the configuration use case; trade‑off is potential write contention during save operations.
- Numeric normalization to `i64`/`f64`: simplifies interop and storage, at the cost of explicit conversions and potential `ValueParse` errors for out‑of‑range values.
- Derive macro ergonomics: declarative mapping with defaults, rename, and ignore keeps module code concise; trade‑off is limited custom conversion hooks (see Open questions).

## Testing strategy

Recommended tiers for automated coverage:

- **Unit tests (core crate)** — exercise each constructor and typed accessor, including numeric overflow/underflow and datetime parsing fallback paths.
- **Integration tests (core + derive)** — define small sample modules under `tests/` to verify `from_confer → load → save` roundtrips and section/key isolation.
- **Compile-time contract tests** — leverage `trybuild` suites in `conferencier-derive` to assert attribute misuse (`default`+`init`, malformed literals, unsupported field types) produces clear compiler errors.
- **Snapshot tests** — for persistence, compare saved TOML string outputs (`save_str`) to golden files to catch accidental format regressions.

Illustrative async roundtrip for the core crate:

```rust
use conferencier::{Confer, Result};
use tempfile::NamedTempFile;
use std::io::Write;

#[tokio::test]
async fn roundtrip_from_file_async() -> Result<()> {
  // Prepare a temporary TOML file
  let mut tf = NamedTempFile::new().expect("tmp");
  write!(tf, "[App]\nname=\"demo\"\n").unwrap();
  let path = tf.into_temp_path();

  // Load asynchronously
  let c = Confer::from_file_async(path.to_str().unwrap()).await?;
  assert_eq!(c.get_string("App", "name").await?, "demo");

  // Save a change and write back
  c.set_integer("App", "port", 8080).await?;
  c.save_file(path.to_str().unwrap()).await?;
  Ok(())
}
```

---
