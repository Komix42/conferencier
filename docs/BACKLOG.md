# Backlog

Known technical debt and ideas for future versions of conferencier.

Unlike the RFCs and implementation reports in this folder, this is a living document: entries are added, changed and removed as the project evolves. An entry is not a commitment. An idea that is taken up gets its own RFC and its entry links to it; the entry is removed once the RFC is implemented. Fixed technical debt is recorded in the [changelog](../CHANGELOG.md) and removed from here.

Each entry states the problem, a possible direction and where it comes from.

## Technical debt

- **Unused `SectionGuard`.** `conferencier/src/section_guard.rs` is compiled but not used anywhere; only `#[allow(dead_code)]` keeps it building without warnings. Derived modules replace whole sections on save, so they no longer need to track the keys they own. Direction: remove the module. Origin: [IMPL-001](./IMPL_001_baseline.md#7-known-limitations--follow-up-ideas).
- **API names.** Public identifiers still carry `Confer`, the working name of the crate, which was already taken on crates.io: `Confer`, `SharedConfer`, `ConferModule`, `SharedConferModule`, `ConferTable`, `ConferError`, `from_confer` and the modules `confer_module` and `confer_table`. "Module" also clashes with Rust modules, and no name says that a module shares runtime state between tasks through its ignored fields, not only values from the TOML file. Almost every public name changes, so the renaming should happen while the crate is at 0.0.x; revisit it after the 0.0.4 release and write an RFC with the final names. Direction, with working names from the discussion:
    - `Confer` → `ConfigStore`. The trait functions take `&ConfigStore` instead of an `Arc` by value, because derived code uses the store only during the call: callers no longer clone the `Arc` for every `load` and `save`, and a `&Arc<ConfigStore>` coerces to `&ConfigStore`. A store is almost always shared by the tasks that build their domains from it. Open question: what the constructors return.
        - `Arc<ConfigStore>`, as today. The typical use needs no `Arc::new`, but every store is an `Arc`, also in a single task, inside an application state that is already shared, or in a `static`. It is also inconsistent with `Default`, which returns the bare store.
        - `ConfigStore`, which the application wraps in an `Arc` once. Conventional and flexible: an `Arc` can be added, but not removed.
        - Working preference: `ConfigStore` as a cheap handle that holds the table in an `Arc` and implements `Clone`, like `reqwest::Client` or `sqlx::Pool`. Constructors return `ConfigStore`, sharing is `store.clone()`, and `SharedConfer` disappears without an `Arc` in the public API. A clone shares the data instead of copying it, which the documentation must say.

      None of the variants needs a `ConfigStore::into_shared` helper.
    - `ConferModule` → `ConfigDomain`, `SharedConferModule<T>` → `SharedDomain<T>`. A domain belongs to one component and owns one section: its loaded fields map to the section, its ignored fields hold runtime state. Weakness: the word is abstract and has other meanings, such as DNS domains or domain-driven design. Alternatives considered:
        - `ConfigUnit` clashes with `()` and unit structs.
        - `ConfigSection` fits the loaded fields, but suggests that the struct holds only what is in the TOML file, and a section with `#[confer(section = ...)]` reads as a tautology.
        - `ConfigNode` suggests a tree, is hard to tell apart from nested tables and means a cluster node in networked applications.
        - `ConfigRealm` is overblown and an established term for authentication realms.
    - `from_confer` → `from_store`. `load` and `save` keep their names; `reload_from_store` and `save_to_store` are redundant, because the argument type already names the store.
    - `ConferTable` → `ConfigTable`, `ConferError` → `Error`. `Result` keeps its name.
    - The public modules `confer_module`, `confer_table`, `error` and `store`: rename them or make them private, since everything is re-exported at the crate root.
    - One naming scheme for the store's constructors and methods: today `from_string` stands next to `load_str` and `save_str`. A method named `from_str` triggers clippy's `should_implement_trait` lint.
    - The feature `with-derive` → `derive`, declared as `derive = ["dep:conferencier-derive"]` so that no implicit `conferencier-derive` feature exists. The feature is enabled by default, so most users are not affected.
    - The attribute `#[confer(...)]` keeps its name. Like `#[serde(...)]`, it abbreviates the crate name, while a generic name such as `#[unit(...)]` could be read by another derive on the same struct.

    The documentation, which speaks of "modules" throughout, moves to the new terms; the RFCs and implementation reports keep the old names. There are no deprecated aliases, since the API is not stable yet; the changelog gets a **Breaking** entry with a table of old and new names. Origin: a discussion of the API names during the 0.0.4 development.

## Ideas

### Derive attributes and field types

- **Expressions in `default`.** `#[confer(default = ...)]` accepts only literals and arrays of literals, so a default cannot refer to a constant or call a function, unlike `#[confer(ignore = <expr>)]`. Direction: accept any Rust expression, evaluated like an `ignore` initializer (in a closure, type-checked by the compiler), and keep the literal forms. Open questions: whether the expression for an `Option<T>` field has type `T` (wrapped in `Some`) or `Option<T>`; whether string literals keep converting to `String` and datetime strings stay validated at compile time. The expression would run on every load in which the key is missing. The README writes the attribute as `default = <literal>`; change it to `<expr>` when this is implemented. Origin: [RFC-0003](./RFC-0003_field_attributes.md#field-attributes) (A3) kept literal-only defaults; raised again in a documentation review.
- **Conversion hooks.** Types the derive does not support, such as maps, enums or newtypes, must be ignored and handled manually. Direction: `#[confer(with = ...)]` with user-defined conversions, or a `#[confer(serde)]` escape hatch for types implementing `Serialize` and `DeserializeOwned`. Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#out-of-scope), [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **More table shapes.** Maps of tables (`BTreeMap<String, T>` ↔ `[App.servers.alpha]`, `[App.servers.beta]`), `Option<Vec<T>>` and `Box<T>` table fields, `default` on table fields, `#[confer(flatten)]`, and detecting table types without `#[confer(table)]`. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Strict mode for unknown keys.** A misspelled key or table is ignored on load, so its field silently falls back to the default, and the next save removes it. Direction: an opt-in `#[confer(deny_unknown)]` that rejects unknown keys and sub-tables on load. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Modules owning nested sections.** `#[confer(section = "App.server")]` names a top-level key that contains a dot. Direction: treat `section` as a path, with rules for which module owns which sub-table. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Hooks around load and save**, for example to compute derived values or check invariants. Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#limitations).

### Store and errors

- **Path-based typed getters and setters on `Confer`**, for reading or writing a value such as `App.server.port` without a module. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Structured error paths.** Errors inside nested tables carry the path as a string in `section`, such as `App.upstream[1]`. Direction: a new `ConferError` variant with a structured path (`ConferError` is `#[non_exhaustive]`, so adding a variant is not breaking). Origin: [RFC-0004](./RFC-0004_nested_tables.md#alternatives-considered).

### Outside the current scope

The README's [Scope](../README.md#scope) lists these as intentionally left out. Taking one up means changing the scope first.

- **Validation** of allowed ranges, patterns or rules across fields, for example through derive attributes. Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#out-of-scope), [IMPL-001](./IMPL_001_baseline.md#7-known-limitations--follow-up-ideas).
- **Hot reload**: watching configuration files and reloading modules automatically. Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#out-of-scope), [IMPL-001](./IMPL_001_baseline.md#7-known-limitations--follow-up-ideas).
- **Format-preserving saves** that keep comments and layout, for example through `toml_edit`. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Layered sources** such as environment variables, command-line arguments or several files. Origin: the README's Scope.
- **Formats other than TOML.** Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#out-of-scope).

### Examples

- An example integrated with an HTTP server, and a command-line workflow for exporting and importing configuration snapshots. Origin: [RFC-0002](./RFC-0002_baseline_examples.md#future-work).
