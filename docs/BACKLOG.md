# Backlog

Known technical debt and ideas for future versions of conferencier.

Unlike the RFCs and implementation reports in this folder, this is a living document: entries are added, changed and removed as the project evolves. An entry is not a commitment. An idea that is taken up gets its own RFC and its entry links to it. Once an entry is done, because its RFC is implemented or the technical debt is fixed, it is replaced by a one-line note under [Done](#done) with the version that resolved it; changes that affect users are also recorded in the [changelog](../CHANGELOG.md).

Each entry states the problem, a possible direction and where it comes from.

## Planned for 0.0.5

These entries are planned for the 0.0.5 release. The API redesign is implemented (see [Done](#done)), so the remaining RFCs use the final names.

1. **Strict mode for unknown keys** ([Derive attributes and field types](#derive-attributes-and-field-types)): RFC-0006, not yet written.
2. **Expressions in `default`** ([Derive attributes and field types](#derive-attributes-and-field-types)): RFC-0007, not yet written.

## Technical debt

- **Temporary section-name migration guard.** Following [RFC-0005](./RFC-0005_api_redesign.md), `ConfigScope` derives in 0.0.5 and later 0.0.x and 0.1.x releases whose implicit section was affected by the old `Confer` prefix rule require an explicit section. Remove the guard in 0.2.0, and warn in its migration notes that a direct upgrade from 0.0.4 bypasses the guard. Origin: RFC-0005 N7.

## Ideas

### Derive attributes and field types

- **Expressions in `default`.** `#[confer(default = ...)]` accepts only literals and arrays of literals, so a default cannot refer to a constant or call a function, unlike `#[confer(ignore = <expr>)]`. Direction: accept any Rust expression, evaluated like an `ignore` initializer (in a closure, type-checked by the compiler), and keep the literal forms. Open questions: whether the expression for an `Option<T>` field has type `T` (wrapped in `Some`) or `Option<T>`; whether string literals keep converting to `String` and datetime strings stay validated at compile time. The expression would run on every load in which the key is missing. The README writes the attribute as `default = <literal>`; change it to `<expr>` when this is implemented. Planned for 0.0.5 (RFC-0007, not yet written). Origin: [RFC-0003](./RFC-0003_field_attributes.md#field-attributes) (A3) kept literal-only defaults; raised again in a documentation review.
- **Conversion hooks.** Types the derive does not support, such as maps, enums or newtypes, must be ignored and handled manually. Direction: `#[confer(with = ...)]` with user-defined conversions, or a `#[confer(serde)]` escape hatch for types implementing `Serialize` and `DeserializeOwned`. Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#out-of-scope), [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **More table shapes.** Maps of tables (`BTreeMap<String, T>` ↔ `[App.servers.alpha]`, `[App.servers.beta]`), `Option<Vec<T>>` and `Box<T>` table fields, `default` on table fields, `#[confer(flatten)]`, and detecting table types without `#[confer(table)]`. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Strict mode for unknown keys.** A misspelled key or table is ignored on load, so its field silently falls back to the default, and the next save removes it. Direction: an opt-in `#[confer(deny_unknown)]` that rejects unknown keys and sub-tables on load. Planned for 0.0.5 (RFC-0006, not yet written). Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Scopes owning nested sections.** `#[confer(section = "App.server")]` names a top-level key that contains a dot. Direction: treat `section` as a path, with rules for which scope owns which sub-table. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Hooks around load and save**, for example to compute derived values or check invariants. Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#limitations).

### Store and errors

- **Path-based typed getters and setters on `ConfigStore`**, for reading or writing a value such as `App.server.port` without a scope. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Structured error paths.** Errors inside nested tables carry the path as a string in `section`, such as `App.upstream[1]`. Direction: a new `Error` variant with a structured path (`Error` is `#[non_exhaustive]`, so adding a variant is not breaking). Origin: [RFC-0004](./RFC-0004_nested_tables.md#alternatives-considered).

### Outside the current scope

The README's [Scope](../README.md#scope) lists these as intentionally left out. Taking one up means changing the scope first.

- **Validation** of allowed ranges, patterns or rules across fields, for example through derive attributes. Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#out-of-scope), [IMPL-001](./IMPL_001_baseline.md#7-known-limitations--follow-up-ideas).
- **Hot reload**: watching configuration files and reloading scopes automatically. Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#out-of-scope), [IMPL-001](./IMPL_001_baseline.md#7-known-limitations--follow-up-ideas).
- **Format-preserving saves** that keep comments and layout, for example through `toml_edit`. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Layered sources** such as environment variables, command-line arguments or several files. Origin: the README's Scope.
- **Formats other than TOML.** Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#out-of-scope).

### Examples

- An example integrated with an HTTP server, and a command-line workflow for exporting and importing configuration snapshots. Origin: [RFC-0002](./RFC-0002_baseline_examples.md#future-work).

## Done

- **Public API redesign**: implemented in 0.0.5 — names, shared store and async boundaries ([RFC-0005](./RFC-0005_api_redesign.md)).
- **Bug: concurrent `save_file` calls can corrupt the file**: fixed in 0.0.5 with a unique temporary file per save.
- **Unused `serde` dependency**: removed in 0.0.5; add it back if an idea such as a `#[confer(serde)]` escape hatch (see [Conversion hooks](#derive-attributes-and-field-types)) needs it.
- **Unused `SectionGuard`**: removed in 0.0.4. Origin: [IMPL-001](./IMPL_001_baseline.md#7-known-limitations--follow-up-ideas).
