# Backlog

Known technical debt and ideas for future versions of conferencier.

Unlike the RFCs and implementation reports in this folder, this is a living document: entries are added, changed and removed as the project evolves. An entry is not a commitment. An idea that is taken up gets its own RFC and its entry links to it; the entry is removed once the RFC is implemented. Fixed technical debt is recorded in the [changelog](../CHANGELOG.md) and removed from here.

Each entry states the problem, a possible direction and where it comes from.

## Technical debt

- **Unused `SectionGuard`.** `conferencier/src/section_guard.rs` is compiled but not used anywhere; only `#[allow(dead_code)]` keeps it building without warnings. Derived modules replace whole sections on save, so they no longer need to track the keys they own. Direction: remove the module. Origin: [IMPL-001](./IMPL_001_baseline.md#7-known-limitations--follow-up-ideas).

## Ideas

### Derive attributes and field types

- **Expressions in `default`.** `#[confer(default = ...)]` accepts only literals and arrays of literals, so a default cannot refer to a constant or call a function, unlike `#[confer(ignore = <expr>)]`. Direction: accept any Rust expression, evaluated like an `ignore` initializer (in a closure, type-checked by the compiler), and keep the literal forms. Open questions: whether the expression for an `Option<T>` field has type `T` (wrapped in `Some`) or `Option<T>`; whether string literals keep converting to `String` and datetime strings stay validated at compile time. The expression would run on every load in which the key is missing. The README already writes the attribute as `default = <expr>`; change it to `<literal>` if this idea is rejected. Origin: [RFC-0003](./RFC-0003_field_attributes.md#field-attributes) (A3) kept literal-only defaults; raised again in a documentation review.
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
