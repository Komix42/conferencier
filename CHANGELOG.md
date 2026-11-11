# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `ConferTable`, a synchronous trait for plain nested configuration values, and `Confer::set_section_table` for atomic replacement of a complete section.
- `#[derive(ConferTable)]` and the `#[confer(table)]` field attribute map nested TOML tables to plain structs. A table field can be `T` (a sub-table such as `[App.server]`), `Option<T>` or `Vec<T>` (an array of tables such as `[[App.upstream]]`), in modules and in other tables, to any depth. An absent `T` is loaded from an empty table, so its fields' defaults apply; an absent `Vec<T>` loads as empty, unlike a scalar `Vec` without `default`; an empty `Vec<T>` is not written on save. Errors inside nested tables name the full path, for example `invalid value at App.upstream[1].weight: value out of range for u32`. One struct can derive both `ConferModule` and `ConferTable`.
- The `nested_tables` example and a "Nested tables" section in the README.
- The README explains what saving does to a hand-written configuration file: comments, key order and value notation are not preserved, and a module's `save` removes unknown keys and sub-tables from its section, including misspelled ones. Saving is recommended mainly for generating an initial configuration file.
- README sections on the scope of the crate, listing what it intentionally leaves out (formats other than TOML, layered sources, hot reload, validation beyond value types and format-preserving saves), on reloading configuration manually, and on keeping runtime state in a module through ignored fields, which later loads keep.
- API documentation for the variants and fields of `ConferError`, the `Result` alias and the `conferencier-derive` crate. The documentation of `ConferModule::load` and `ConferModule::save` now says what they change and that they lock the module, so a task must not call them while it holds a guard of that module; the README explains this as well.
- `docs/BACKLOG.md` collects known technical debt and ideas for future versions, which were previously spread over the future-work sections of the RFCs.

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
