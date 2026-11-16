# Backlog

Known technical debt and ideas for future versions of conferencier.

Unlike the RFCs and implementation reports in the version folders such as [`devel-0.0.x/`](./devel-0.0.x/), this is a living document: entries are added, changed and removed as the project evolves. An entry is not a commitment. An idea that is taken up gets its own RFC and its entry links to it, unless the release plan explicitly says otherwise. Once an entry is done, it is replaced by a one-line note under [Done](#done) with the version that resolved it; changes that affect users are also recorded in the [changelog](../CHANGELOG.md).

Each entry states the problem, a possible direction and where it comes from.

## Planned for 0.0.7

Focus on presenting the existing library, not new configuration features:

1. **Redesign the user documentation.** Rewrite and reorganize the READMEs and supporting documentation.

No RFCs will be written for 0.0.7. The item is described under [Documentation](#documentation).

## Technical debt

- **Temporary section-name migration guard.** Following [RFC-0005](./devel-0.0.x/RFC-0005_api_redesign.md), `ConfigScope` derives in 0.0.5 and later 0.0.x and 0.1.x releases whose implicit section was affected by the old `Confer` prefix rule require an explicit section. Remove the guard in 0.2.0, and warn in its migration notes that a direct upgrade from 0.0.4 bypasses the guard. Origin: RFC-0005 N7.

## Ideas

### Derive attributes and field types

- **Compile-time checked datetime macro.** With [RFC-0007](./devel-0.0.x/RFC-0007_default_expressions.md), a datetime default is written `"2024-01-01T00:00:00Z".parse().unwrap()`, so an invalid string panics on load instead of failing to compile, as the removed literal form did. Direction: a convenience macro such as `conferencier::datetime!("...")` that validates the string at compile time; usable in `default`, `ignore` and ordinary code. Origin: the RFC-0007 discussion; datetime defaults are expected to be rare.
- **Generated `Default` from `confer` attributes.** With [RFC-0007](./devel-0.0.x/RFC-0007_default_expressions.md) (M4), `#[confer(table, default)]` calls the table type's `Default`, and the standard `#[derive(Default)]` does not know `#[confer(default = ...)]`, so a type with `#[confer(default = 8080)] port: u16` gets `port == 0`. Direction: an opt-in way to generate a `Default` that uses each field's `confer` default (fields without one would need `Default` or fail to compile). Origin: the RFC-0007 discussion.
- **Conversion hooks.** Types the derive does not support, such as maps, enums or newtypes, must be ignored and handled manually. Direction: `#[confer(with = ...)]` with user-defined conversions, or a `#[confer(serde)]` escape hatch for types implementing `Serialize` and `DeserializeOwned`. Origin: [RFC-0001](./devel-0.0.x/RFC-0001_baseline_architecture.md#out-of-scope), [RFC-0004](./devel-0.0.x/RFC-0004_nested_tables.md#out-of-scope--future-work).
- **More table shapes.** Maps of tables (`BTreeMap<String, T>` ↔ `[App.servers.alpha]`, `[App.servers.beta]`), `Option<Vec<T>>` and `Box<T>` table fields, `#[confer(flatten)]`, and detecting table types without `#[confer(table)]`. Origin: [RFC-0004](./devel-0.0.x/RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Scopes owning nested sections.** `#[confer(section = "App.server")]` names a top-level key that contains a dot. Direction: treat `section` as a path, with rules for which scope owns which sub-table. Origin: [RFC-0004](./devel-0.0.x/RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Hooks around load and save**, for example to compute derived values or check invariants. Origin: [RFC-0001](./devel-0.0.x/RFC-0001_baseline_architecture.md#limitations).

### Store and errors

- **Inspecting unknown keys without rejecting them.** Strict scopes fail on the first unknown key, while lenient scopes currently provide no direct way to report leftovers for application logging or diagnostics. Direction: a non-mutating API that lists unmapped keys without changing lenient loading; define how it handles nested tables, strict and lenient table types, and manually implemented `ConfigTable`s before proposing an RFC. Origin: review feedback on [RFC-0006](./devel-0.0.x/RFC-0006_strict_unknown_keys.md).
- **Path-based typed getters and setters on `ConfigStore`**, for reading or writing a value such as `App.server.port` without a scope. Origin: [RFC-0004](./devel-0.0.x/RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Structured error paths.** Errors inside nested tables carry diagnostic paths as strings, such as `App.upstream[1]`; [RFC-0006](./devel-0.0.x/RFC-0006_strict_unknown_keys.md#error-paths) makes this notation unambiguous and adds public formatting helpers, but does not add a structured path type. Direction: consider a structured type if callers need to inspect path segments without parsing display notation. Origin: [RFC-0004](./devel-0.0.x/RFC-0004_nested_tables.md#alternatives-considered), [RFC-0006](./devel-0.0.x/RFC-0006_strict_unknown_keys.md#out-of-scope).
- **Caller-specified permissions for new files.** [RFC-0010](./devel-0.0.x/RFC-0010_save_file_permissions.md) preserves the mode bits of an existing Unix target, but a new target gets the operating system's default permissions, so a private file must be pre-created by the caller. Direction: an option to request a Unix mode for a file created by `save_file`, applied to the temporary file before writing; decide what it means on Windows before proposing an RFC. Origin: review feedback on the RFC-0010 draft.
- **Preserving Windows access control on replacement.** [RFC-0010](./devel-0.0.x/RFC-0010_save_file_permissions.md) copies no DACL or other access settings on Windows, so a replaced file gets the directory's inherited access, which can be broader. Direction: copy the target's DACL to the temporary file before writing, which needs Windows APIs or a dependency. Origin: RFC-0010 R1.
- **Synchronizing saved files.** `save_file` performs no file or directory synchronization, so a delayed write error can go unreported and a crash after success can leave the target incomplete. Direction: `sync_all` on the temporary file before rename reports more write errors without an API change; directory synchronization after rename is less portable. Origin: [RFC-0009](./devel-0.0.x/RFC-0009_save_file_replacement.md#durability-and-alternatives) R1.
- **Rejecting reserved temporary-file target names.** [RFC-0009](./devel-0.0.x/RFC-0009_save_file_replacement.md) R5 documents target names that can resolve to a temporary file as unsupported, without checking them. Direction: reject at least names beginning with `.conferencier.` (ASCII case-insensitively) with a clear error; a check cannot detect every filesystem alias. Origin: the RFC-0009 review; deferred because no ordinary configuration file uses such names.

### Outside the current scope

The README's [Scope](../README.md#scope) lists these as intentionally left out. Taking one up means changing the scope first.

- **Validation** of allowed ranges, patterns or rules across fields, for example through derive attributes. Origin: [RFC-0001](./devel-0.0.x/RFC-0001_baseline_architecture.md#out-of-scope), [IMPL-001](./devel-0.0.x/IMPL_001_baseline.md#7-known-limitations--follow-up-ideas).
- **Hot reload**: watching configuration files and reloading scopes automatically. Origin: [RFC-0001](./devel-0.0.x/RFC-0001_baseline_architecture.md#out-of-scope), [IMPL-001](./devel-0.0.x/IMPL_001_baseline.md#7-known-limitations--follow-up-ideas).
- **Format-preserving saves** that keep comments and layout, for example through `toml_edit`. Origin: [RFC-0004](./devel-0.0.x/RFC-0004_nested_tables.md#out-of-scope--future-work).
- **Layered sources** such as environment variables, command-line arguments or several files. Origin: the README's Scope.
- **Formats other than TOML.** Origin: [RFC-0001](./devel-0.0.x/RFC-0001_baseline_architecture.md#out-of-scope).

### Documentation

- **Redesign the user documentation.** The README has accumulated attribute details, migration notes, edge cases and operational guidance, making it harder to understand the crate at a glance. Direction: rewrite the introduction and quick start as a clear showcase of the crate's purpose and typical use; split detailed reference material and migration guidance into appropriate documentation, keeping important limitations discoverable through links. Move the design records (RFCs and implementation reports) in `docs/` to `docs/devel-0.0.x/`, preserving historical records and updating links. Keep the root and crate READMEs aligned, preserve runnable examples and doctest coverage, and keep the documentation usable on GitHub, crates.io and docs.rs; review the derive crate's README for its distinct audience too. The detailed structure will be decided separately. Origin: the post-0.0.5 planning discussion.

### Examples

- **Hyper HTTP server example.** A complete, runnable example configuring a Hyper-based web server and demonstrating several components. Keep Hyper specific to the example, not a runtime dependency of the library. The detailed scenario will be decided separately. Origin: [RFC-0002](./devel-0.0.x/RFC-0002_baseline_examples.md#future-work).
- **Configuration snapshot workflow.** A command-line workflow for exporting and importing configuration snapshots. Origin: [RFC-0002](./devel-0.0.x/RFC-0002_baseline_examples.md#future-work).

## Done

- **Reliable `ConfigStore::save_file` replacement and target validation**: implemented in 0.0.6 ([RFC-0009](./devel-0.0.x/RFC-0009_save_file_replacement.md)).
- **Preserve existing Unix file mode bits when saving**: implemented in 0.0.6 ([RFC-0010](./devel-0.0.x/RFC-0010_save_file_permissions.md)).
- **Reject `section` on table-only derives**: implemented in 0.0.5 ([RFC-0008](./devel-0.0.x/RFC-0008_table_section.md)).
- **Expressions in `default` and one rule for missing keys**: implemented in 0.0.5 ([RFC-0007](./devel-0.0.x/RFC-0007_default_expressions.md)).
- **Strict unknown-key rejection and diagnostic path formatting**: implemented in 0.0.5 ([RFC-0006](./devel-0.0.x/RFC-0006_strict_unknown_keys.md)).
- **Public API redesign**: implemented in 0.0.5 — names, shared store and async boundaries ([RFC-0005](./devel-0.0.x/RFC-0005_api_redesign.md)).
- **Bug: concurrent `save_file` calls can corrupt the file**: fixed in 0.0.5 with a unique temporary file per save.
- **Unused `serde` dependency**: removed in 0.0.5; add it back if an idea such as a `#[confer(serde)]` escape hatch (see [Conversion hooks](#derive-attributes-and-field-types)) needs it.
- **Unused `SectionGuard`**: removed in 0.0.4. Origin: [IMPL-001](./devel-0.0.x/IMPL_001_baseline.md#7-known-limitations--follow-up-ideas).
