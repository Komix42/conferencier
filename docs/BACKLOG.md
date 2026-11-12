# Backlog

Known technical debt and ideas for future versions of conferencier.

Unlike the RFCs and implementation reports in this folder, this is a living document: entries are added, changed and removed as the project evolves. An entry is not a commitment. An idea that is taken up gets its own RFC and its entry links to it. Once an entry is done, because its RFC is implemented or the technical debt is fixed, it is replaced by a one-line note under [Done](#done) with the version that resolved it; changes that affect users are also recorded in the [changelog](../CHANGELOG.md).

Each entry states the problem, a possible direction and where it comes from.

## Planned for 0.0.5

These entries are planned for the 0.0.5 release. The renaming comes first, so that the other RFCs use the final names.

1. **API names** ([Technical debt](#technical-debt)): [RFC-0005](./RFC-0005_api_names.md), draft.
2. **Strict mode for unknown keys** ([Derive attributes and field types](#derive-attributes-and-field-types)): RFC-0006, not yet written.
3. **Expressions in `default`** ([Derive attributes and field types](#derive-attributes-and-field-types)): RFC-0007, not yet written.
4. **Unused `serde` dependency** ([Technical debt](#technical-debt)): no RFC needed.

## Technical debt

- **API names.** Public identifiers still carry `Confer`, the working name of the crate, which was already taken on crates.io: `Confer`, `SharedConfer`, `ConferModule`, `SharedConferModule`, `ConferTable`, `ConferError`, `from_confer` and the modules `confer_module` and `confer_table`. "Module" also clashes with Rust modules, and no name says that a module shares runtime state between tasks through its ignored fields, not only values from the TOML file. Almost every public name changes, so the renaming should happen while the crate is at 0.0.x. [RFC-0005](./RFC-0005_api_names.md) proposes the final names for 0.0.5, starting from the direction below. Direction, with working names from the discussion:
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
    - The public modules `confer_module`, `confer_table` and `error`: rename them or make them private, since everything is re-exported at the crate root. The module `store` is already private.
    - One naming scheme for the store's constructors and methods: today `from_string` stands next to `load_str` and `save_str`. A method named `from_str` triggers clippy's `should_implement_trait` lint.
    - The feature `with-derive` → `derive`, declared as `derive = ["dep:conferencier-derive"]` so that no implicit `conferencier-derive` feature exists. The feature is enabled by default, so most users are not affected.
    - The attribute `#[confer(...)]` keeps its name. Like `#[serde(...)]`, it abbreviates the crate name, while a generic name such as `#[unit(...)]` could be read by another derive on the same struct.

    The documentation, which speaks of "modules" throughout, moves to the new terms; the RFCs and implementation reports keep the old names. There are no deprecated aliases, since the API is not stable yet; the changelog gets a **Breaking** entry with a table of old and new names. Origin: a discussion of the API names during the 0.0.4 development.
- **Unused `serde` dependency.** `conferencier` depends on `serde` with the `derive` feature, but no code uses it; TOML values are converted through the `toml` crate. Its `derive` feature is the only reason why every build of `conferencier` compiles the `serde_derive` proc-macro. Direction: remove it, and add it back if an idea such as a `#[confer(serde)]` escape hatch (see [Conversion hooks](#derive-attributes-and-field-types)) needs it. Origin: a review of the workspace after the 0.0.4 release.
- **Bug: concurrent `save_file` calls can corrupt the file.** The temporary file of the atomic write has a fixed name derived from the target (`context.toml` → `context.toml.tmp`), so concurrent saves to the same path write into the same temporary file and rename it while another save is still writing to it. In an experiment with four tasks that each changed their own section and saved one shared store (300 rounds), 338 of 1200 saves returned an error and the target file was left unparsable 32 times. Direction: a unique temporary name per save, for example from the process id and a counter. The guarantee, stated in the rustdoc of `save_file`, is then that the file on disk is always one complete snapshot; which of several concurrent saves wins stays unspecified, and an application that needs an order serializes its own saves — coordinating writers is its job, not the store's. A plain bug fix with no API change, recorded in the changelog under *Fixed*; no RFC needed, and whether it lands in 0.0.5 is decided separately. Origin: a review of `save_file` against a multi-component application during the RFC-0005 discussion.

## Ideas

### Derive attributes and field types

- **Expressions in `default`.** `#[confer(default = ...)]` accepts only literals and arrays of literals, so a default cannot refer to a constant or call a function, unlike `#[confer(ignore = <expr>)]`. Direction: accept any Rust expression, evaluated like an `ignore` initializer (in a closure, type-checked by the compiler), and keep the literal forms. Open questions: whether the expression for an `Option<T>` field has type `T` (wrapped in `Some`) or `Option<T>`; whether string literals keep converting to `String` and datetime strings stay validated at compile time. The expression would run on every load in which the key is missing. The README writes the attribute as `default = <literal>`; change it to `<expr>` when this is implemented. Planned for 0.0.5 (RFC-0007, not yet written). Origin: [RFC-0003](./RFC-0003_field_attributes.md#field-attributes) (A3) kept literal-only defaults; raised again in a documentation review.
- **Conversion hooks.** Types the derive does not support, such as maps, enums or newtypes, must be ignored and handled manually. Direction: `#[confer(with = ...)]` with user-defined conversions, or a `#[confer(serde)]` escape hatch for types implementing `Serialize` and `DeserializeOwned`. Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#out-of-scope), [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **More table shapes.** Maps of tables (`BTreeMap<String, T>` ↔ `[App.servers.alpha]`, `[App.servers.beta]`), `Option<Vec<T>>` and `Box<T>` table fields, `default` on table fields, `#[confer(flatten)]`, and detecting table types without `#[confer(table)]`. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Strict mode for unknown keys.** A misspelled key or table is ignored on load, so its field silently falls back to the default, and the next save removes it. Direction: an opt-in `#[confer(deny_unknown)]` that rejects unknown keys and sub-tables on load. Planned for 0.0.5 (RFC-0006, not yet written). Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
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

## Done

- **Unused `SectionGuard`**: removed in 0.0.4. Origin: [IMPL-001](./IMPL_001_baseline.md#7-known-limitations--follow-up-ideas).
