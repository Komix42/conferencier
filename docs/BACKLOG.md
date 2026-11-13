# Backlog

Known technical debt and ideas for future versions of conferencier.

Unlike the RFCs and implementation reports in this folder, this is a living document: entries are added, changed and removed as the project evolves. An entry is not a commitment. An idea that is taken up gets its own RFC and its entry links to it. Once an entry is done, because its RFC is implemented or the technical debt is fixed, it is replaced by a one-line note under [Done](#done) with the version that resolved it; changes that affect users are also recorded in the [changelog](../CHANGELOG.md).

Each entry states the problem, a possible direction and where it comes from.

## Planned for 0.0.5

The planned RFCs for 0.0.5 are implemented; see [Done](#done).

## Technical debt

- **Temporary section-name migration guard.** Following [RFC-0005](./RFC-0005_api_redesign.md), `ConfigScope` derives in 0.0.5 and later 0.0.x and 0.1.x releases whose implicit section was affected by the old `Confer` prefix rule require an explicit section. Remove the guard in 0.2.0, and warn in its migration notes that a direct upgrade from 0.0.4 bypasses the guard. Origin: RFC-0005 N7.
- **`save_file` does not preserve the target file's permissions.** The temporary file is created with default permissions (the directory's inherited ACL on Windows, the process umask on Unix) and the rename replaces the target together with its permissions. A file restricted to its owner can therefore become readable by others after a save, for example a file with explicit owner-only access in a directory that grants read access to the Users group on Windows, or a `0600` file under umask `022` on Unix. The content is also exposed through the temporary file before the rename. Reproduced on Windows; the Unix case follows from the same code path. Direction: create the temporary file with the target's permissions (on Unix, open it with the target's mode before writing; on Windows, copy the security descriptor or use `ReplaceFileW`), or document that callers restrict the directory instead. Origin: the 0.0.5 review; present since the original atomic save.

## Ideas

### Derive attributes and field types

- **Compile-time checked datetime macro.** With [RFC-0007](./RFC-0007_default_expressions.md), a datetime default is written `"2024-01-01T00:00:00Z".parse().unwrap()`, so an invalid string panics on load instead of failing to compile, as the removed literal form did. Direction: a convenience macro such as `conferencier::datetime!("...")` that validates the string at compile time; usable in `default`, `ignore` and ordinary code. Origin: the RFC-0007 discussion; datetime defaults are expected to be rare.
- **Generated `Default` from `confer` attributes.** With [RFC-0007](./RFC-0007_default_expressions.md) (M4), `#[confer(table, default)]` calls the table type's `Default`, and the standard `#[derive(Default)]` does not know `#[confer(default = ...)]`, so a type with `#[confer(default = 8080)] port: u16` gets `port == 0`. Direction: an opt-in way to generate a `Default` that uses each field's `confer` default (fields without one would need `Default` or fail to compile). Origin: the RFC-0007 discussion.
- **Conversion hooks.** Types the derive does not support, such as maps, enums or newtypes, must be ignored and handled manually. Direction: `#[confer(with = ...)]` with user-defined conversions, or a `#[confer(serde)]` escape hatch for types implementing `Serialize` and `DeserializeOwned`. Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#out-of-scope), [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **More table shapes.** Maps of tables (`BTreeMap<String, T>` ↔ `[App.servers.alpha]`, `[App.servers.beta]`), `Option<Vec<T>>` and `Box<T>` table fields, `#[confer(flatten)]`, and detecting table types without `#[confer(table)]`. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Scopes owning nested sections.** `#[confer(section = "App.server")]` names a top-level key that contains a dot. Direction: treat `section` as a path, with rules for which scope owns which sub-table. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Hooks around load and save**, for example to compute derived values or check invariants. Origin: [RFC-0001](./RFC-0001_baseline_architecture.md#limitations).

### Store and errors

- **Inspecting unknown keys without rejecting them.** Strict scopes fail on the first unknown key, while lenient scopes currently provide no direct way to report leftovers for application logging or diagnostics. Direction: a non-mutating API that lists unmapped keys without changing lenient loading; define how it handles nested tables, strict and lenient table types, and manually implemented `ConfigTable`s before proposing an RFC. Origin: review feedback on [RFC-0006](./RFC-0006_strict_unknown_keys.md).
- **Path-based typed getters and setters on `ConfigStore`**, for reading or writing a value such as `App.server.port` without a scope. Origin: [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Structured error paths.** Errors inside nested tables carry diagnostic paths as strings, such as `App.upstream[1]`; [RFC-0006](./RFC-0006_strict_unknown_keys.md#error-paths) makes this notation unambiguous and adds public formatting helpers, but does not add a structured path type. Direction: consider a structured type if callers need to inspect path segments without parsing display notation. Origin: [RFC-0004](./RFC-0004_nested_tables.md#alternatives-considered), [RFC-0006](./RFC-0006_strict_unknown_keys.md#out-of-scope).

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

- **Expressions in `default` and one rule for missing keys**: implemented in 0.0.5 ([RFC-0007](./RFC-0007_default_expressions.md)).
- **Strict unknown-key rejection and diagnostic path formatting**: implemented in 0.0.5 ([RFC-0006](./RFC-0006_strict_unknown_keys.md)).
- **Public API redesign**: implemented in 0.0.5 — names, shared store and async boundaries ([RFC-0005](./RFC-0005_api_redesign.md)).
- **Bug: concurrent `save_file` calls can corrupt the file**: fixed in 0.0.5 with a unique temporary file per save.
- **Unused `serde` dependency**: removed in 0.0.5; add it back if an idea such as a `#[confer(serde)]` escape hatch (see [Conversion hooks](#derive-attributes-and-field-types)) needs it.
- **Unused `SectionGuard`**: removed in 0.0.4. Origin: [IMPL-001](./IMPL_001_baseline.md#7-known-limitations--follow-up-ideas).
