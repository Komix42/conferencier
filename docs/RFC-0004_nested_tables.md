# RFC-0004: Nested tables via `ConferTable`

Date: 2026-11-09
Status: Implemented
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.0.4
Requires: [RFC-0003](./RFC-0003_field_attributes.md)
Amends: [RFC-0001](./RFC-0001_baseline_architecture.md)
Amended by: [RFC-0005](./RFC-0005_api_redesign.md), [RFC-0006](./RFC-0006_strict_unknown_keys.md), [RFC-0007](./RFC-0007_default_expressions.md), [RFC-0008](./RFC-0008_table_section.md)

## Document Scope

This RFC specifies support for nested TOML tables in derived configuration modules: a struct field can hold another struct that is stored as a sub-table (`[App.server]`), an optional sub-table, or an array of tables (`[[App.upstream]]`). It defines the new public API (`ConferTable` trait and derive, `#[confer(table)]` field attribute, `Confer::set_section_table`), the load/save semantics, error reporting for nested locations, compile-time diagnostics, and an internal refactor of `#[derive(ConferModule)]` that the feature builds on.

It amends the following parts of [RFC-0001](./RFC-0001_baseline_architecture.md), whose text stays unchanged (see [Documentation deliverables](#documentation-deliverables)):

- the [API surface summary](./RFC-0001_baseline_architecture.md#api-surface-summary): the [trait surface](./RFC-0001_baseline_architecture.md#canonical-type-aliases-and-trait-surface) (new `ConferTable` trait), the [core crate](./RFC-0001_baseline_architecture.md#core-crate-conferencier) raw accessors (`set_section_table`) and the [derive crate summary](./RFC-0001_baseline_architecture.md#derive-crate-conferencier-derive) (new derive, `table` attribute, table field shapes);
- the "Nested table projections" item listed as [out of scope](./RFC-0001_baseline_architecture.md#out-of-scope) and "nested structures" in [Open questions / next steps](./RFC-0001_baseline_architecture.md#open-questions--next-steps);
- [Interaction with derived modules](./RFC-0001_baseline_architecture.md#interaction-with-derived-modules) and its [Save semantics](./RFC-0001_baseline_architecture.md#interaction-with-derived-modules);
- the [Error model reference](./RFC-0001_baseline_architecture.md#error-model-reference): the `section` of an error inside a nested table holds a path, and [Type normalisation and fallbacks](./RFC-0001_baseline_architecture.md#type-normalisation-and-fallbacks) covers table types;
- the [procedural macro reference](./RFC-0001_baseline_architecture.md#conferencier-derive-procedural-macro): supported field types, attributes, loading and saving;
- [Edge cases and behavior](./RFC-0001_baseline_architecture.md#edge-cases-and-behavior);
- [Limitations](./RFC-0001_baseline_architecture.md#limitations): custom user types can be mapped as table types.

The baseline is RFC-0001 as amended by [RFC-0003](./RFC-0003_field_attributes.md), including its field attributes and direct construction of modules. RFC-0003 is a prerequisite and may be implemented and released beforehand; the two RFCs do not have to ship in the same release. This RFC extends that baseline to table values and introduces whole-section snapshots and atomic load/save. Everything not explicitly changed here retains the baseline behavior.

## Synopsis

```rust
use conferencier::{Confer, ConferModule, ConferTable, Result};

#[derive(ConferModule)]
#[confer(section = "App")]
struct AppConfig {
    name: String,
    #[confer(table)]
    server: Server,
    #[confer(table)]
    tls: Option<Tls>,
    #[confer(table, rename = "upstream")]
    upstreams: Vec<Upstream>,
}

#[derive(ConferTable)]
struct Server {
    #[confer(default = "0.0.0.0")]
    host: String,
    port: u16,
}

#[derive(ConferTable)]
struct Tls {
    cert: String,
    key: String,
}

#[derive(ConferTable)]
struct Upstream {
    url: String,
    #[confer(default = 1)]
    weight: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let store = Confer::from_file("app.toml")?;
    let app = AppConfig::from_confer(store.clone()).await?;

    app.write().await.server.port = 9090;

    AppConfig::save(&app, store.clone()).await?;
    store.save_file("app.toml").await?;
    Ok(())
}
```

Input written by hand, using TOML 1.1 multi-line inline tables:

```toml
[App]
name = "demo"
server = {
    port = 8080,  # host falls back to its default
}
tls = {
    cert = "/etc/app/cert.pem",
    key = "/etc/app/key.pem",
}
upstream = [
    { url = "http://10.0.0.1" },
    { url = "http://10.0.0.2", weight = 3 },
]
```

Output after `save` + `save_file` (layout produced by the `toml` serializer, see [Saving](#saving)):

```toml
[App]
name = "demo"

[App.server]
host = "0.0.0.0"
port = 9090

[App.tls]
cert = "/etc/app/cert.pem"
key = "/etc/app/key.pem"

[[App.upstream]]
url = "http://10.0.0.1"
weight = 1

[[App.upstream]]
url = "http://10.0.0.2"
weight = 3
```

## Table of Contents

- [Document Scope](#document-scope)
- [Synopsis](#synopsis)
- [Motivation](#motivation)
- [Goals and non-goals](#goals-and-non-goals)
- [Glossary](#glossary)
- [User-facing API](#user-facing-api)
  - [The `ConferTable` trait](#the-confertable-trait)
  - [Derive macro `ConferTable`](#derive-macro-confertable)
  - [The `table` field attribute](#the-table-field-attribute)
  - [Supported field shapes](#supported-field-shapes)
  - [Store addition: `set_section_table`](#store-addition-set_section_table)
- [Semantics](#semantics)
  - [Loading](#loading)
  - [Saving](#saving)
  - [Ownership of nested data](#ownership-of-nested-data)
  - [Paths and error reporting](#paths-and-error-reporting)
  - [Ignored fields inside tables](#ignored-fields-inside-tables)
- [Changes to `ConferModule`](#changes-to-confermodule)
- [Compile-time diagnostics](#compile-time-diagnostics)
- [Implementation notes (non-normative)](#implementation-notes-non-normative)
- [Compatibility](#compatibility)
- [Alternatives considered](#alternatives-considered)
- [Out of scope / future work](#out-of-scope--future-work)
- [Milestones](#milestones)
- [Testing strategy](#testing-strategy)
- [Documentation deliverables](#documentation-deliverables)
- [Resolved questions](#resolved-questions)

## Motivation

- **Readable nested configuration.** TOML has long supported nested tables through dotted keys and table headers, and inline tables were available for compact values. TOML 1.1 (supported since conferencier 0.0.3) allows inline tables to span multiple lines, contain comments and end with a trailing comma, so grouped settings can be written next to the rest of their section.
- **Modelling in Rust.** Related settings (`server`, `tls`, `upstream`) belong in their own types. Today they must be flattened into one section with prefixed keys (`server_host`, `server_port`) or handled manually, and the same group cannot be reused in several modules.
- **Current rejection is in the derive only.** The store already loads and saves nested tables as raw `toml::Value::Table`; only `#[derive(ConferModule)]` rejects non-scalar field types.

## Goals and non-goals

Goals:

- Map a struct field to a sub-table, an optional sub-table or an array of tables, to arbitrary depth.
- Reuse the field attributes (`rename`, `default`, `ignore`) and conversion rules inside nested types.
- Report errors with the full location of the offending key (`App.upstream[1].weight`).
- Keep all existing public signatures. Preserve existing module behavior except for the explicit changes listed in [Changes to `ConferModule`](#changes-to-confermodule): all-or-nothing load, atomic save and a consistent non-table-section check.
- Keep the store two-level (`section`, `key`); nesting is handled by the derive layer.

Non-goals (see [Out of scope / future work](#out-of-scope--future-work)): flattening, maps of tables, `Option<Vec<T>>` table fields, modules owning nested sections, format-preserving saves, path-based typed getters on `Confer`.

## Glossary

- **Table type** — a type implementing `ConferTable`, usually via `#[derive(ConferTable)]`. It is a plain value without its own lock or section.
- **Table field** — a field marked with `#[confer(table)]` whose type is one of the [supported shapes](#supported-field-shapes) around a table type.
- **Loaded field / ignored field** — as defined in RFC-0003: a loaded field is any field not marked `ignore`, including table fields; an ignored field is not affected by the configuration file.
- **Owning module** — the `ConferModule` whose section (directly or transitively) contains a table field.
- **Path** — a diagnostic string identifying a table inside the store, e.g. `App`, `App.server`, `App.upstream[1]`. See [Paths and error reporting](#paths-and-error-reporting).

## User-facing API

### The `ConferTable` trait

New module `conferencier::confer_table`, re-exported at the crate root next to the derive macro of the same name (the same pattern as `ConferModule`, so `use conferencier::ConferTable;` imports both):

```rust
use conferencier::toml::Table;
use conferencier::Result;

#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as a nested configuration table",
    note = "derive it with `#[derive(ConferTable)]` or implement `ConferTable` manually"
)]
pub trait ConferTable: Sized {
    /// Builds a value from `table`. `path` identifies `table` in error messages.
    fn from_table(path: &str, table: Table) -> Result<Self>;
    /// Converts `self` into the complete table to store. `path` is used for errors only.
    fn to_table(&self, path: &str) -> Result<Table>;
}
```

Contract (applies to derived and manual implementations):

- `from_table` receives an owned snapshot, so values can be moved out without cloning. Unknown keys may be ignored.
- Errors about keys of `table` itself use `path` as the `section` of `ConferError::{MissingKey, TypeMismatch, ValueParse}`; errors inside nested values use their nested path (see [Paths and error reporting](#paths-and-error-reporting)).
- `to_table` returns the complete table. Keys it does not return are removed from the store when the owning module is saved.
- The trait is synchronous and has no `Send`/`Sync`/`'static` bounds; the owning module's `ConferModule` bounds already require them transitively.
- Manual implementations are supported, but the trait signature (including the `toml::Table` type, re-exported as `conferencier::toml::Table`) may change before 0.1, for example if staged loading ([Alternatives considered](#alternatives-considered), item 8) is adopted. The trait's rustdoc says so.

Manual implementations make the trait an extension point for types that cannot derive it:

```rust
use conferencier::toml::{Table, Value};
use conferencier::{ConferError, ConferTable, Result};

struct Endpoint(std::net::SocketAddr);

impl ConferTable for Endpoint {
    fn from_table(path: &str, mut table: Table) -> Result<Self> {
        let raw = match table.remove("addr") {
            Some(Value::String(raw)) => raw,
            Some(other) => {
                return Err(ConferError::type_mismatch(path, "addr", "string", other.type_str()));
            }
            None => return Err(ConferError::missing_key(path, "addr")),
        };
        raw.parse()
            .map(Endpoint)
            .map_err(|err| ConferError::value_parse(path, "addr", err.to_string()))
    }

    fn to_table(&self, _path: &str) -> Result<Table> {
        let mut table = Table::new();
        table.insert("addr".into(), Value::String(self.0.to_string()));
        Ok(table)
    }
}
```

### Derive macro `ConferTable`

`#[derive(ConferTable)]` is exported by `conferencier-derive` and re-exported by `conferencier` under the `with-derive` feature.

- Applies to structs with named fields; enums, tuple structs and unit structs are rejected like in `ConferModule`.
- Field attributes and supported field types are the same as for `ConferModule` (RFC-0001 as amended by RFC-0003), plus `#[confer(table)]`. Fields may nest table types to any depth.
- `from_table` applies the direct-construction pattern of RFC-0003 A5 to a table value: first convert all loaded fields, then initialize ignored fields and construct `Self`. Loaded fields do not need `Default`. Initializers use the isolated synchronous expression context of RFC-0003 A1–A2, here within `from_table`; their lifecycle is specified by I1 below.
- Key naming and duplicate key detection are the same as for `ConferModule`: the field name (raw identifiers without `r#`) or `rename`. Duplicate keys of loaded fields are a compile error; ignored fields have no key (RFC-0003 A1).
- Struct-level attributes: `section` is accepted and ignored, so one struct can derive both `ConferModule` and `ConferTable` (used as its own module and nested elsewhere). No other struct-level attributes are accepted.
- The generated code is synchronous, contains only the trait implementation (no inherent items), and refers to TOML types through `conferencier::toml`, so the user's crate does not need a direct `toml` dependency.
- `#[derive(ConferModule)]` does **not** implement `ConferTable`. Using a module type as a table field requires deriving both.

### The `table` field attribute

`#[confer(table)]` marks a field of a `ConferModule` or `ConferTable` struct as a table field.

| Combination | Allowed | Reason |
| --- | --- | --- |
| `table` + `rename` | yes | Renames the sub-table key. |
| `table` + `default` | no | Defaults belong to the fields of the table type; an absent table already falls back to them. |
| `table` + `ignore` | no | Contradictory; use `ignore` alone for fields the configuration file does not affect. |
| `table` twice | no | Duplicate attribute, like other attributes. |

The attribute is explicit because a procedural macro cannot tell whether an arbitrary type implements a trait. Inferring it later is a non-breaking change; the reverse would not be.

### Supported field shapes

`T` below is any type implementing `ConferTable`.

| Field type | TOML representation | Key absent on load | Present with wrong TOML type | Save |
| --- | --- | --- | --- | --- |
| `T` | table (`[App.server]` or inline) | `T::from_table(path, empty table)`: inner defaults and `None`s apply, inner required keys fail with `MissingKey` | `TypeMismatch`, expected `table` | Always written; an empty table renders as a bare header `[App.server]`. |
| `Option<T>` | table | `None` | `TypeMismatch`, expected `table` | `Some` → table; `None` → key removed. |
| `Vec<T>` | array of tables (`[[App.upstream]]` or inline array) | empty `Vec` | `TypeMismatch`, expected `array`; a non-table element → `ValueParse` with its index | Non-empty → array of tables; empty → key removed. |

The derive provides built-in handling only for `T`, `Option<T>` and `Vec<T>`. It rejects syntactically recognized unsupported shapes such as `Option<Vec<T>>`, `Vec<Option<T>>`, `Option<Option<T>>`, `Vec<Vec<T>>`, references and tuples with a shape diagnostic. `Box<T>` and maps have no built-in table representation; unless the exact field type implements `ConferTable` (where Rust's coherence rules permit a manual implementation), they fail with the trait diagnostic described under [Compile-time diagnostics](#compile-time-diagnostics).

Type aliases are opaque to the derive's syntax-based shape check. An alias to a type that implements `ConferTable` can be used as `T`; an alias to an unsupported wrapper is not expanded into a supported container and does not gain a `ConferTable` implementation automatically.

Rationale for the absent/empty rules:

- An absent `T` is treated as an empty table so that adding a new group with defaulted fields does not break existing configuration files. The same reasoning makes an absent `Vec<T>` an empty array; table fields have no `default` attribute to express it otherwise.
- An empty `Vec<T>` is omitted on save because `key = []` defines a static array, and TOML forbids adding `[[App.upstream]]` entries to it later. A user editing the saved file would get a parse error (verified with `toml` 1.2.1).
- The two rules depend on each other: since an empty `Vec<T>` is not written, an absent key must load as empty, otherwise a save/load roundtrip would fail with `MissingKey`. This differs from scalar vectors (`Vec<String>` without `default` fails with `MissingKey`), and the documentation says so.
- `Option<Vec<T>>` is postponed. It adds little over `Vec<T>`, because an absent key already means an empty list. `Some(empty)` would have to be written as `key = []`, which blocks adding `[[...]]` entries by hand later. It can be added later without breaking changes.

### Store addition: `set_section_table`

```rust
impl Confer {
    /// Replaces the whole table stored at `section` in a single write.
    pub async fn set_section_table(&self, section: &str, table: Table) -> Result<()>;
}
```

- Creates the section when it does not exist.
- If `section` holds a non-table value, returns `TypeMismatch { section, key: "<section>", expected: "table", found }` and leaves the store unchanged (same rule as `set_value` and `add_section`).
- Readers never observe a partially replaced section.
- Complements the existing `get_section_table`; derived modules use it for `save`.

## Semantics

### Loading

- **L1** — Values inside table types follow the conversion and fallback rules of RFC-0001 ("Type normalisation and fallbacks"), with the table's path in place of the section name.
- **L2** — Absent keys are handled per [Supported field shapes](#supported-field-shapes); for non-table fields the RFC-0001 rules apply (`default`, `Option` → `None`, otherwise `MissingKey`).
- **L3** — Both standard tables (`[App.server]`, dotted keys) and inline tables (single- or multi-line) are accepted; after parsing they are indistinguishable.
- **L4** — Unknown keys are ignored at every level.
- **L5** — Fields are converted in declaration order, depth first; the first conversion error is returned. For `Vec<T>`, each array element is checked and fully converted to `T` before the next element is checked.
- **L6** — A table field that fails to load fails the whole `load`/`from_confer` of the owning module (see [Changes to `ConferModule`](#changes-to-confermodule) for atomicity).

### Saving

- **S1** — The owning module serializes its whole section, including all nested tables, and replaces the section in one store write (`set_section_table`).
- **S2** — Nested tables and arrays of tables are written according to [Supported field shapes](#supported-field-shapes); non-table fields follow RFC-0001 (`None` → key removed, numeric normalization, `ValueParse` for values that do not fit `i64`).
- **S3** — Conversion errors during save (e.g. a `u64` above `i64::MAX` inside a nested table) are reported with the nested path, and the store is left unchanged.
- **S4** — The textual layout is produced by the `toml` serializer and is not part of the contract. With `toml` 1.2 nested tables are written as `[App.server]` headers and arrays of tables as `[[App.upstream]]`. Hand-written inline layout and comments are not preserved; this is unchanged from RFC-0001.

### Ownership of nested data

- **O1** — A module owns its section recursively: every sub-table and array of tables below it belongs to the module, whether mapped by a field or not.
- **O2** — Unknown keys and unknown sub-tables anywhere inside the section are removed on save. This already holds before this RFC for keys holding sub-tables and is now documented explicitly. Data not owned by the module must live in a different top-level section.
- **O3** — `#[confer(section = "App.server")]` keeps naming a single top-level key that contains a dot (serialized as `["App.server"]`), not a nested path. Modules owning nested sections are out of scope.

### Paths and error reporting

The existing error variants are reused; their `section` field carries the path of the table containing the failing key. No variant or `Display` format changes.

- **E1** — The path of a module's section is the section name, verbatim (unchanged).
- **E2** — A key segment added to a nested table path is written bare if it is non-empty and consists only of `A-Z a-z 0-9 _ -`; otherwise it is quoted and escaped as a TOML basic string (`App."my key"`). This rule applies to the nested path held in `ConferError.section`; the final `ConferError.key` remains unchanged and is displayed by the existing `Display` implementation without quoting.
- **E3** — The path of an element of an array of tables is `{array path}[{index}]` with a zero-based index (`App.upstream[1]`).
- **E4** — Errors about a table field's own value use the parent path and the unchanged field key; errors inside a nested value use the nested path. The path formatting rule in E2 applies only to path segments, not to the final key passed separately to `ConferError`.
- **E5** — Paths are diagnostic strings, not addresses accepted by the store API.

Examples:

| Situation | Message |
| --- | --- |
| `[App.server]` lacks `port` | `missing key App.server.port` |
| `server = 5` | `expected table at App.server but found integer` |
| `[App.upstream]` written instead of `[[App.upstream]]` | `expected array at App.upstream but found table` |
| `upstream = [{ url = "a" }, "b"]` | `invalid value at App.upstream: expected array elements of type table, found string (at index 1)` |
| `weight = -1` in the second upstream | `invalid value at App.upstream[1].weight: value out of range for u32` |
| `"my key" = { port = "bad" }` where `port` is an integer field | `expected integer at App."my key".port but found string` |

### Ignored fields inside tables

- **I1** — Table types have value semantics: every successful load of the owning module builds new values through `from_table`. Each ignored field in a table value is initialized once, from `Default::default()` or `ignore = <expr>`, after all loaded fields of that table value have converted successfully. Its previous runtime state is replaced only if the owning module load succeeds. If a later table or sibling field fails, any side effects already performed by initializers in temporary table values cannot be rolled back, although the module itself remains unchanged.
- **I2** — Ignored fields of the module struct itself retain RFC-0003 A1 semantics: they are initialized once in `from_confer`, after all of the module's loaded fields convert successfully, and never touched by `load`.

Preserving runtime state inside nested values would require matching old and new values (impossible for array elements in general) and a staged-load associated type in the trait; see [Alternatives considered](#alternatives-considered).

## Changes to `ConferModule`

`#[derive(ConferModule)]` is reimplemented on top of the same table conversion used by `ConferTable`, instead of issuing one typed getter/setter call per field. This supersedes the description of generated code in RFC-0001 [Interaction with derived modules](./RFC-0001_baseline_architecture.md#interaction-with-derived-modules); the ownership rules described there are unchanged. The `ConferModule` trait signature is unchanged.

- **R1 — Snapshot load.** `load` and `from_confer` read the whole section once under a single store read lock and convert all loaded fields without holding any lock. `load` then assigns them under a single module write lock; `from_confer` constructs the module directly from the converted values, following RFC-0003 A5.
- **R2 — All-or-nothing load.** If any field fails, `load` returns the error and the module, including all nested values and ignored fields, keeps its previous state. Before this RFC, fields preceding the failing one were already updated. Initializers for temporary nested values may already have run before a later field fails; their external side effects are not rolled back (I1).
- **R3 — Atomic save.** `save` first converts the module into a complete section table under the module read lock, releases it, then replaces the section with `set_section_table`. Concurrent readers of a whole-section snapshot never observe a half-saved module, and a conversion error leaves the store unchanged. Conversion errors therefore take precedence over a non-table section error if both conditions are present. Before this RFC, keys were written one by one and earlier fields were persisted before a failing one.
- **R4 — Lock discipline.** Generated code never holds a module lock and a store lock at the same time.
- **R5 — Non-table section.** `load` and `from_confer` check the section before conversion and return `TypeMismatch { section, key: "<section>", expected: "table", found }` if it exists but is not a table. This check also applies to modules with no loaded fields; before this RFC, their generated `load` was a no-op and did not inspect the section. `save` returns the same mismatch if conversion succeeds and the existing section is not a table; a conversion error takes precedence as specified in R3.
- **R6 — Unchanged behavior.** Section naming, key naming, defaults, `Option` handling, numeric range checks, datetime fallbacks, removal of unknown keys on save, and error variants and message formats for existing field types remain as specified in RFC-0001 as amended by RFC-0003. Error precedence may change as specified in R3 and R5. The test suite as updated by RFC-0003, including its new tests, must pass without further modification, except the `fail_unsupported_type` `trybuild` snapshot, which is updated for the extended message (see [Compile-time diagnostics](#compile-time-diagnostics)).
- **R7 — Field bounds.** Fields no longer need to be cloned on save; values are converted under the read lock.

These atomicity guarantees apply to individual snapshots and writes, not to a transaction spanning the module and the store. Multiple independent getter calls do not share a snapshot and may observe different versions.

## Compile-time diagnostics

| Misuse | Diagnostic |
| --- | --- |
| `#[confer(table)]` on a type that does not implement `ConferTable` (including scalars such as `String`) | Trait error with the `on_unimplemented` message: `` `Foo` cannot be used as a nested configuration table `` plus the derive hint. |
| `#[confer(table)]` combined with `default` or `ignore` | `#[confer(table)] cannot be combined with #[confer(<attr>)]` |
| `#[confer(table)]` on a syntactically recognized unsupported shape (e.g. `Option<Vec<T>>`, `Vec<Option<T>>`) | `#[confer(table)] supports T, Option<T> and Vec<T>` |
| `#[confer(table)]` on `Box<T>`, a map, or another type that does not implement `ConferTable` | Trait error with the `on_unimplemented` message, unless the exact field type has a manual `ConferTable` implementation |
| Duplicate `#[confer(table)]` | `duplicate #[confer(table)] attribute` |
| Custom type without `#[confer(table)]` | Existing `` unsupported field type `Foo` `` error, extended with `; nested structs require #[confer(table)] on the field and #[derive(ConferTable)] on the type` |
| Unsupported struct-level attribute on `ConferTable` | `unsupported attribute on struct for #[derive(ConferTable)]` |
| `#[derive(ConferTable)]` on an enum or a struct without named fields | Same errors as `ConferModule`, naming `ConferTable`. |

Messages are indicative; the exact wording is fixed by the `trybuild` snapshots.
When both derives are applied to one struct, each derive invocation validates the field attributes independently; invalid attributes may therefore produce a diagnostic from each derive. The compiler is not required to deduplicate them.

## Implementation notes (non-normative)

- **Shared codegen.** One generator produces, for a list of fields, (a) a block converting an owned `Table` plus a path into a tuple of loaded field values and (b) a block building a `Table` from a value. `ConferTable` uses both directly; `ConferModule` wraps them with the snapshot and lock handling from R1–R3. Module-level ignored initializers are not evaluated by `load` (I2); constructing temporary nested values follows I1.
- **Initializer hygiene.** Extend the isolated synchronous initializer context from RFC-0003 to generated `from_table` implementations. It must work for generic table structs and without adding inherent items.
- **Hidden runtime helpers** in `conferencier::__private` (names illustrative):
  - `read_section(&Confer, section) -> Result<Table>` — snapshot under one read lock; empty table when absent; `TypeMismatch` when not a table (R5).
  - `child_path(parent, key) -> String` and `element_path(path, index) -> String` implementing E2/E3.
  - Re-exports of the `value_conversion` functions, plus helpers to extract an array and validate one table element at a time (or equivalent generated code). Do not eagerly convert the whole array to `Vec<Table>` before converting its elements to `T`; that would violate the depth-first first-error rule in L5.
- **Parser.** `parse_module` becomes parameterised by the derive kind (for error messages and the `section` attribute). `FieldType` gains a table variant holding the inner type and its container (`Plain`, `Option`, `Vec`).
- **Paths are allocated** for every nested table during load and save. This is negligible for configuration data and keeps manual implementations simple.
- **Generics.** No trait bounds are added automatically. A generic table field (`#[confer(table)] inner: T`) needs a user-written `where T: ConferTable` bound.
- **Recursion.** Recursive types work through `Vec<Self>` (e.g. a tree of nodes). Parsed input is subject to the TOML parser's nesting limit. This RFC introduces no separate depth limit for programmatically constructed tables passed to `from_table` or for values converted by `to_table` during saving; these need not have passed through the parser.

## Compatibility

- **Additive API:** `ConferTable` trait and derive, the `confer_table` module, `Confer::set_section_table`, the `#[confer(table)]` attribute.
- **Behavior changes** for existing modules (listed under "Changed" in the changelog): all-or-nothing `load` (R2), atomic `save` and conversion-error precedence (R3), a consistent non-table-section check even for modules with no loaded fields (R5), and the extended "unsupported field type" message. None of them changes a public signature.
- **Lockstep crate release.** Code generated for this feature uses new `__private` helpers and requires the matching core release. Both crates share the workspace version and are published together; this synchronizes the core and derive crates, not the release schedules of the two RFCs.
- **MSRV** stays 1.88 (`#[diagnostic::on_unimplemented]` is stable since 1.78).
- **TOML files** written by earlier versions load unchanged. Files using nested tables could already be loaded by the raw store API.

## Alternatives considered

1. **Implicit detection without `#[confer(table)]`.** Treating every unknown type as a table produces confusing trait errors for genuinely unsupported types (e.g. `HashMap`) and cannot be reverted later. Explicit first, possibly implicit later.
2. **Nested types as `ConferModule`s.** A module is an `Arc<RwLock<_>>` with an async API that owns a section; a nested value lives inside its parent's lock and is a plain value. Wrong abstraction.
3. **serde (`Serialize`/`DeserializeOwned`) for nested types.** Little code, and it would cover maps and enums, but it introduces a second attribute vocabulary, loses compile-time validated defaults and gives coarser errors. Kept as a possible `#[confer(serde)]` escape hatch.
4. **Path-based typed getters on `Confer` as the primary mechanism.** Useful for manual access but does not give typed structs; possible future addition.
5. **A new error variant with a structured path.** Since 0.0.4 `ConferError` is `#[non_exhaustive]`, so a variant could be added without a breaking change. Reusing `MissingKey`, `TypeMismatch` and `ValueParse` with the path in `section` means that code matching these variants also handles errors from nested tables without changes, and the `Display` formats stay the same. Structured access to the path can be added later.
6. **Borrowed `from_table(&Table)`.** Forces cloning every value; the owned snapshot is already a clone.
7. **Absent `Vec<T>` → `MissingKey`** (consistent with scalar `Vec` without `default`). Rejected because table fields have no `default` and the absent-is-empty rule mirrors `T`.
8. **Preserving ignored fields of nested values across loads.** Requires a staged-load associated type in the trait and an identity rule for array elements; rejected in favor of value semantics (I1).
9. **One conversion trait for every field type.** A blanket conversion trait could remove the explicit `#[confer(table)]` marker, but it would make table intent less visible and would not provide the shape-specific diagnostics or opt-in boundary of this RFC. The explicit marker is retained; a later conversion hook can be considered separately.

## Out of scope / future work

- `#[confer(flatten)]` — embed the fields of a table type directly in the parent table (key collisions detectable only at runtime).
- Maps of tables (`BTreeMap<String, T>` ↔ `[App.servers.alpha]`, `[App.servers.beta]`).
- Modules owning nested sections (`section` as a real path) with per-sub-table ownership rules.
- Format-preserving saves (comments, inline layout), e.g. via `toml_edit`.
- Path-based typed getters/setters on `Confer`.
- `#[confer(serde)]`, `#[confer(with = ...)]` conversion hooks.
- `default` on table fields, `Option<Vec<T>>` table fields (postponed; adding them is non-breaking), `Box<T>` and other container shapes.
- Implicit detection of table types.
- A strict mode, e.g. `#[confer(deny_unknown)]`, that rejects unknown keys on load instead of ignoring them on load and removing them on save (O2). It would catch typos such as `[App.sever]`, which today silently fall back to defaults and are removed by the next save.

## Milestones

1. **Core** — `set_section_table`, `ConferTable` trait and re-exports, `__private` helpers (section snapshot, paths, conversions); unit tests.
2. **`ConferModule` refactor** — table-based load/save using direct construction as specified in RFC-0003 A5 (R1–R7); tests as updated by RFC-0003 unchanged and green (R6), new atomicity tests.
3. **`ConferTable`** — derive macro, `#[confer(table)]` for all three shapes, diagnostics; integration and `trybuild` tests.
4. **Documentation** — README sections, changelog, example, cross-reference in RFC-0001; RFC status → Implemented (see [Resolved questions](#resolved-questions), item 4).

## Testing strategy

- **Core unit tests:** `set_section_table` (create, replace, non-table section → `TypeMismatch` with store unchanged); path quoting (bare keys, keys needing quotes, escaping, array indices).
- **Integration tests (`module_integration` or a new `nested_tables` test):**
  - Roundtrip of all three shapes, including three levels of nesting and `rename` on table fields.
  - Input written with TOML 1.1 multi-line inline tables with comments and trailing commas; saved output reloads to an equal module.
  - Absent `T` uses inner defaults; absent `T` with a required inner key → `MissingKey` with the nested path; absent `Option<T>` → `None`; absent `Vec<T>` → empty.
  - Save: `None` removes the key; empty `Vec<T>` omits the key; unknown nested keys and sub-tables are removed.
  - Every error row of [Paths and error reporting](#paths-and-error-reporting), asserting both the variant fields and the `Display` output.
  - A `Vec<T>` whose first table has a missing required field and whose next element is not a table returns the first element's nested `MissingKey`, proving depth-first error order (L5).
  - Ignored fields inside table types are re-initialized on load (I1); module-level ignored fields are preserved (I2).
  - R2: a failing `load` leaves the module, including nested ignored state, unchanged. R3: a `u64` range error during `save` leaves the store unchanged.
  - R1/R2 concurrency: a store writer atomically alternates two whole-section versions with two correlated fields while generated `from_confer` and `load` read them. Every constructed module must contain one complete version, and a concurrent module reader holding one read lock must never observe mixed fields.
  - R3 concurrency: a writer alternates both correlated module fields under one write lock and persists them through generated `save`. A concurrent reader using `get_section_table` must observe one complete section version, never a mixture. These concurrency tests exercise the generated operations, not just `set_section_table`, and use coordinated tasks rather than timing-based sleeps.
  - R3/R5: test that a conversion error during save takes precedence over a non-table section; after successful conversion, the non-table section yields `TypeMismatch`.
  - R5: a derived module whose section holds a non-table value fails `load` and `save` with `TypeMismatch`, including a module with no loaded fields.
  - A recursive table type through `Vec<Self>` (e.g. a tree of nodes) roundtrips.
  - A table type implementing neither `Default` nor `Clone` compiles and roundtrips (required `Datetime` without `default` is covered by RFC-0003).
  - A struct deriving both `ConferModule` and `ConferTable`, with and without `section`.
  - A manual `ConferTable` implementation used as a table field.
  - Hygiene: the existing hygiene test is extended to the new generated code of both derives (fields named `path`, `table`, `store`, `module`, `value`, `key`), and a local `mod toml {}` breaks neither derive.
  - A new test target is declared in `conferencier/Cargo.toml` with `required-features = ["with-derive"]`, like `module_integration`, so the `--no-default-features` CI job still builds.
- **`trybuild`:** passing cases covering all shapes and nesting, including a generic table struct with explicit bounds and an ignored-field initializer; failing cases for every row of [Compile-time diagnostics](#compile-time-diagnostics) and an initializer referring to generated `from_table` locals (e.g. `table` or `path`); updated snapshot of `fail_unsupported_type`. Only one case asserts the `on_unimplemented` trait error: trait-error output changes most between rustc releases, and CI runs `trybuild` on the latest stable.
- **CI gates:** `cargo fmt`, `clippy -D warnings`, tests and `cargo doc` with `-D warnings`, as in the existing workflow.

## Documentation deliverables

- Root `README.md` and `conferencier/README.md` (runs as a doctest):
  - A nested tables section with a short example, the field shape table, and a warning that `section = "App.server"` does not denote a nested table (O3).
  - The difference between an absent table `Vec<T>`, which loads as empty, and an absent scalar `Vec` without `default`, which fails with `MissingKey`.
  - Ignored fields inside table types are re-initialized by every successful load, and side effects of their initializers are not rolled back when the load fails (I1).
  - The existing "Saving configuration" section extended: unknown keys are removed recursively, including inside mapped sub-tables (O1–O2).
- `conferencier-derive/README.md`: mention `#[derive(ConferTable)]` and `#[confer(table)]`.
- `CHANGELOG.md` (Unreleased):
  - "Added" for the new API;
  - "Changed" for R2/R3/R5 and the diagnostic message.
- A runnable example (e.g. `examples/nested_tables.rs` with `required-features = ["with-derive"]`).
- Rustdoc of `ConferTable`, including the stability note for manual implementations.
- RFC-0001 stays unchanged, as specified in [RFC-0003](./RFC-0003_field_attributes.md#documentation-deliverables). When this RFC is marked Implemented, it is added to the `Amended by:` header line of RFC-0001. The amended parts are listed in [Document Scope](#document-scope) only.

## Resolved questions

1. **Absent `Vec<T>`** → empty `Vec`, as required by the rule that an empty `Vec<T>` is not written on save (see [Supported field shapes](#supported-field-shapes)). The difference from scalar vectors is documented.
2. **A `T` table whose fields are all absent or `None`** → still written as a bare header (`[App.server]`), which keeps the structure visible and editable.
3. **`section` on `ConferTable`** → accepted and ignored, so one struct can derive both macros. A derive cannot see other derives on the same struct, so rejecting it would rule the combination out.
4. **Implementation report** → if the implementation follows this RFC, the status is changed to Implemented and the changes are recorded in the changelog. An `IMPL-002` report is written only if the implementation has to depart from this RFC; it then describes and justifies the differences.
5. **`Option<Vec<T>>` table fields** → postponed (see [Supported field shapes](#supported-field-shapes)).
