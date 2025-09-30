# IMPL-001: Baseline Implementation Report

Date: 2025-09-21
Status: Closed
Author: Komix42 <komix[at]ignore.cz>

## 1. Executive Summary

The initial delivery of the `conferencier` workspace implements an asynchronous, TOML-backed configuration hub accompanied by a `#[derive(ConferModule)]` procedural macro. The core crate (`conferencier`) exposes a shared `Confer` store that wraps a `tokio::sync::RwLock<toml::Table>`, provides typed getters/setters, and handles persistence to strings and files. The companion derive crate (`conferencier-derive`) generates async load/save flows for module structs, honouring declarative metadata for section naming, defaults, renames, and runtime-only fields. Comprehensive unit, integration, and trybuild tests validate the behaviour.

## 2. Workspace Structure

```
conferencier/
├── Cargo.toml            # Workspace manifest (members: core + derive)
├── conferencier/         # Core library crate
│   ├── src/
│   │   ├── lib.rs
│   │   ├── store.rs
│   │   ├── value_conversion.rs
│   │   ├── confer_module.rs
│   │   ├── error.rs
│   │   └── section_guard.rs
│   └── tests/
│       ├── core_roundtrip.rs
│       └── module_integration.rs
├── conferencier-derive/  # Procedural macro crate
│   ├── src/
│   │   ├── lib.rs
│   │   ├── parser.rs
│   │   ├── model.rs
│   │   ├── codegen.rs
│   │   └── crate_path.rs
│   └── tests/
│       ├── trybuild.rs
│       └── trybuild/*.rs
└── docs/
    └── IMPL_001_baseline.md (this document)
```

## 3. Core Crate (`conferencier`)

### 3.1 Primary Types

| Item | Description |
| --- | --- |
| `Confer` | Runtime configuration hub wrapping `tokio::sync::RwLock<toml::Table>`. |
| `SharedConfer` | Type alias `Arc<Confer>` to share the store across tasks. |
| `SharedConferModule<T>` | Alias `Arc<RwLock<T>>`, used by derived modules. |
| `ConferError` / `Result<T>` | Error surface with variants for IO, parse, serialization, missing keys, type mismatches, and value parsing failures. |

### 3.2 Persistence API

- Constructors: `Confer::new`, `Confer::from_string`, `Confer::from_file` (blocking), `Confer::from_file_async` (async).
- Load operations: `load_str`, `load_file` replace the in-memory table atomically.
- Save operations: `save_str`, `save_file` (asynchronous) serialize under a read lock. `save_file` writes atomically via temp-file + rename.

### 3.3 Typed Accessors

The store exposes asynchronous getters/setters for scalar and vector TOML types:

- Scalars: `get_string`, `get_integer`, `get_float`, `get_boolean`, `get_datetime`.
- Vectors: `get_string_vec`, `get_integer_vec`, `get_float_vec`, `get_boolean_vec`, `get_datetime_vec`.
- Matching setters convert typed inputs into TOML `Value`s and upsert section entries.

All typed getters funnel through `value_conversion.rs`, which performs type checks, numeric normalisation (to `i64`/`f64`), datetime fallbacks (string → `Datetime`), and detailed error reporting using `ConferError::TypeMismatch` or `ConferError::ValueParse`.

### 3.4 Section Management Utilities

- `add_section`, `remove_section`, `remove_key`, `list_sections`, `list_keys`, and `section_exists` ensure idempotent updates and safe enumeration.
- `section_guard.rs` (presently unused at runtime) provides a `SectionGuard` helper that can reconcile known keys; public API is suppressed via `#[allow(dead_code)]` until needed by higher-level logic.

### 3.5 Module Trait and Private Support

`confer_module.rs` defines the `ConferModule` trait that the derive macro targets. A hidden `__private` module re-exports `async_trait`, `Arc`, `RwLock`, and provides `new_shared_module` for constructing shared module handles without leaking the internal synchronization strategy.

### 3.6 Feature Flags

- Default features include `with-derive`, which re-exports the procedural macro (`pub use conferencier_derive::ConferModule`).
- Consumers can opt out by disabling default features and depending on `conferencier-derive` directly if proc-macro support is undesired.

## 4. Derive Crate (`conferencier-derive`)

### 4.1 Module Responsibilities

| Module | Responsibility |
| --- | --- |
| `parser.rs` | Parses `DeriveInput`, validates attributes (`section`, `rename`, `default`, `init`, `ignore`), classifies field types (scalar/container), and materialises default/init token streams. |
| `model.rs` | DTO layer capturing parsed modules (`Module`, `Field`, `FieldType`, enum descriptors). |
| `codegen.rs` | Generates async implementations for `from_confer`, `load`, and `save`, including conversion glue for typed accessors and default/missing-key handling. |
| `crate_path.rs` | Resolves the path to the core crate using `proc-macro-crate`, supporting both self and renamed dependencies. |
| `lib.rs` | Macro entry point; orchestrates parse → model → codegen pipeline. |

### 4.2 Attribute Semantics

- Struct level: `#[confer(section = "Srv")]` overrides the default section name (otherwise type name minus leading `Confer`).
- Field level:
  - `#[confer(rename = "...")]` maps to custom key names.
  - `#[confer(default = ...)]` accepts literals (`"text"`, `42`, `true`, `3.14`, `[1, 2]`, `"2024-01-01T00:00:00Z"`). Parser ensures type compatibility and injects code to build concrete Rust values (`String::from`, numeric casts, datetime parse via `FromStr`).
  - `#[confer(init = "<expr>")]` runs before load (mutually exclusive with `default`).
  - `#[confer(ignore)]` excludes a field from load/save while still honouring `default`/`init` initialisation.

### 4.3 Generated Behaviour

- `from_confer` seeds fields using defaults/initializers, constructs `Arc<RwLock<Self>>`, and immediately calls `load`.
- `load` fetches section keyed values via typed getters, applies container semantics (`Option`, `Vec`, `Option<Vec>`), and handles missing keys:
  - Required fields (no default, not optional) propagate `ConferError::MissingKey`.
  - Optional fields default to `None` unless overridden.
  - Datetime vectors fall back to string parsing.
- `save` synchronises the TOML section with the in-memory state:
  - Ensures the section exists (`add_section`).
  - Clones module fields under a read guard, then writes values via typed setters.
  - Removes entries for `Option::None` and prunes unknown keys in the module-owned section, preventing drift from manual edits.

### 4.4 Safety and Error Propagation

- All generated async functions return `Result<()>` or `Result<SharedConferModule<Self>>`, bubbling up `ConferError` from the core.
- Numeric conversions guard against overflow/underflow; unsigned values are checked before re-casting to `i64`.
- Datetime parsing uses `toml::value::Datetime::from_str` with explicit `.expect()` only during compile-time literal validation (safe due to compile-time checks).

## 5. Testing Strategy

### 5.1 Core Unit Tests (`conferencier/src/store.rs`)

- Cover creation (`Confer::new`), load/save round-trips (`save_file_overwrites_existing`), typed conversions (string/float/datetime vectors), section management, and error cases (`MissingKey`).

### 5.2 Integration Tests (`conferencier/tests`)

- `core_roundtrip.rs`: Exercises concurrent read/write behaviour and serialization round-trips.
- `module_integration.rs`: Validates derive-generated modules handling of rename, defaults, options, ignored fields, and persistence effects on the underlying store.

### 5.3 Trybuild Suite (`conferencier-derive/tests/trybuild`)

- `pass_basic.rs`: Happy path verifying complex attribute combinations (defaults, renames, vectors, options, datetime parsing).
- Failure cases:
  - `fail_duplicate_keys.rs`: Duplicate key detection via rename.
  - `fail_unsupported_type.rs`: Ensures unsupported field types emit compiler errors.
  - `fail_conflicting_attrs.rs`: Confirms `default` + `init` conflicts are rejected.
- Driver `tests/trybuild.rs` runs the pass/fail matrix; requires dev-dependency on `conferencier` (with default features disabled) to compile fixtures.

### 5.4 Continuous Validation

- `cargo test` (workspace) currently passes with warnings suppressed (dead-code allowances in `section_guard.rs`).

## 6. Example Suite

- `examples/basic_usage.rs`: Demonstrates a single module (`AppSettings`) loading from an inline TOML string, applying defaults and option semantics, mutating fields, and persisting back to the store. The file is heavily commented and includes a `cargo run --example basic_usage` hint.
- `examples/advanced_usage.rs`: Showcases two cooperating modules (`ServerProfile`, `FeatureToggles`) that share a `Confer` instance, exercise renames, vector defaults, optional vectors, and ignored runtime fields, and persist a reconciled snapshot to disk. The example loads its initial state from `examples/data/advanced_config.toml` and saves to `target/advanced_usage_snapshot.toml`.
- `examples/data/advanced_config.toml`: Source configuration consumed by the advanced scenario to mirror real-world file-based workflows.

Both examples rely on the default `with-derive` feature and bring the macro into scope via `use conferencier::ConferModule;`, enabling the ergonomic `#[derive(ConferModule)]` syntax described in RFC-0002. They also import the trait privately (`use conferencier::confer_module::ConferModule as _;`) so the generated `from_confer`, `load`, and `save` methods remain available without exposing the trait in the public surface of downstream crates.

## 7. Known Limitations & Follow-up Ideas

- Complex/nested user types remain out-of-scope; runtime-only fields must use `#[confer(ignore)]` or manual persistence.
- `SectionGuard` is scaffolded but unused; future work could integrate it to avoid manual pruning logic in codegen.
- Formatting in `save_file` relies on `toml::to_string`, so comments and ordering are not preserved.
- Potential enhancements include custom conversion hooks (`#[confer(with = ...)]`), validation attributes, and hot-reload support.

## 8. Usage Notes

```rust
use conferencier::{Confer, ConferModule, Result};
use conferencier::confer_module::ConferModule as _;

#[derive(ConferModule)]
#[confer(section = "App")]
struct Settings {
    #[confer(default = "localhost")]
    host: String,
    #[confer(default = 8080)]
    port: u16,
    note: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let store = Confer::from_string("[App]\nport = 3000\n")?;
    let module = Settings::from_confer(store.clone()).await?;

    {
        let mut guard = module.write().await;
        guard.note = Some("Updated via conferencier".into());
    }

    Settings::save(&module, store.clone()).await?;
    Ok(())
}
```

## 9. Revision History

| Date | Change |
| --- | --- |
| 2025-09-29 | Documented baseline example suite and adoption of RFC-0002. |
| 2025-09-29 | Initial baseline implementation report. |
