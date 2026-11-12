# RFC-0006: Strict mode for unknown keys

Date: 2026-11-12
Status: Draft
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.0.5

## Document Scope

This RFC adds an opt-in struct attribute, `#[confer(deny_unknown_keys)]`, for `#[derive(ConfigScope)]` and `#[derive(ConfigTable)]`. A struct with the attribute rejects keys and sub-tables of its table that no loaded field maps, instead of ignoring them. It adds the error variant `Error::UnknownKey`. Structs without the attribute behave as before: unknown keys are ignored on load and removed on save ([RFC-0004](./RFC-0004_nested_tables.md) O2). Saving does not change.

The proposal starts from the [Strict mode for unknown keys](./BACKLOG.md#derive-attributes-and-field-types) entry of the backlog, first listed as future work in [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work). It uses the names from [RFC-0005](./RFC-0005_api_redesign.md). Decisions made while drafting are recorded in [Resolved questions](#resolved-questions).

## Synopsis

```rust
use conferencier::{ConfigScope, ConfigStore, ConfigTable, Error};

#[derive(ConfigScope)]
#[confer(section = "App", deny_unknown_keys)]
struct AppConfig {
    #[confer(default = 8080)]
    port: u16,
    #[confer(table)]
    server: Server,
}

#[derive(ConfigTable)]
#[confer(deny_unknown_keys)]
struct Server {
    #[confer(default = "localhost")]
    host: String,
}

let store: ConfigStore = "[App]\nprot = 9090\n".parse()?;
match AppConfig::from_store(&store) {
    Err(Error::UnknownKey { section, key }) => assert_eq!((section.as_str(), key.as_str()), ("App", "prot")),
    _ => unreachable!(),
}
```

Without the attribute, `prot = 9090` is ignored, `port` falls back to `8080`, and the next `save` deletes `prot`. With it, `from_store` and `load` fail with `unknown key App.prot`.

## Table of Contents

- [Document Scope](#document-scope)
- [Synopsis](#synopsis)
- [Motivation](#motivation)
- [Goals and non-goals](#goals-and-non-goals)
- [The attribute](#the-attribute)
- [Semantics](#semantics)
- [The error](#the-error)
- [Compile-time diagnostics](#compile-time-diagnostics)
- [Implementation notes (non-normative)](#implementation-notes-non-normative)
- [Compatibility](#compatibility)
- [Alternatives considered](#alternatives-considered)
- [Out of scope](#out-of-scope)
- [Testing](#testing)
- [Documentation deliverables](#documentation-deliverables)
- [Resolved questions](#resolved-questions)

## Motivation

- **Typos are silent.** A misspelled key such as `prot = 9090` or a misspelled table such as `[App.sever]` is ignored on load. The field falls back to its default or loads as `None`, and nothing tells the user that the configuration file has no effect.
- **The next save destroys the evidence.** A scope's `save` replaces its whole section, so the misspelled key disappears from a saved file (O2). The README describes this as a known pitfall in [Saving configuration](../README.md#saving-configuration).
- **Ignored fields look configurable.** A runtime field marked `#[confer(ignore)]` has no key. A user who writes it into the file sees no effect and no error.
- **The lenient default stays useful.** Applications that read configuration written by a newer version of themselves, or that tolerate leftovers, should keep working. Strictness is therefore opt-in and chosen per struct.

## Goals and non-goals

Goals:

- An opt-in, per-struct way to reject keys that no loaded field maps, in scopes and in table types at any depth.
- An error that names the full path of the unknown key, like the existing errors.
- No change for structs without the attribute, no change to saving and no change to the `ConfigScope` and `ConfigTable` traits.

Non-goals:

- Detecting unknown top-level sections. A store holds the sections of many scopes, and no single scope knows them all (see [Out of scope](#out-of-scope)).
- Suggestions for similar names ("did you mean `port`?").
- Strictness inherited from a scope by the table types it contains (see [Alternatives considered](#alternatives-considered), item 2).

## The attribute

- **U1 — Struct attribute.** `#[confer(deny_unknown_keys)]` is accepted on structs deriving `ConfigScope` or `ConfigTable`, next to `section`, in the same or a separate `#[confer(...)]` attribute. It takes no value. A struct deriving both macros applies it to both implementations.
- **U2 — Known keys.** The known keys of a struct are the keys of its loaded fields: scalar fields and `#[confer(table)]` fields, after `rename`. An ignored field has no key, so a key named like an ignored field is unknown. With `rename`, the Rust field name is not a known key.
- **U3 — Per type, not inherited.** The attribute applies only to the table of the struct that carries it. A table type nested in a strict scope stays lenient unless the table type has the attribute itself, and a strict table type is strict in every scope that uses it. Each element of an array of tables is checked by its element type.

## Semantics

- **U4 — Load.** `from_store`, `load` and `from_table` of a strict struct fail with `Error::UnknownKey` if the table contains a key that is not known (U2). Sub-tables, arrays of tables and inline tables are keys as well. A missing section or an absent table is an empty table and passes.
- **U5 — Precedence.** The check runs after the table itself has been obtained, and before any field is converted or checked: missing keys, type mismatches, out-of-range values and nested tables are all reported after unknown keys. For a scope, a section that is not a table is reported first ([RFC-0004](./RFC-0004_nested_tables.md) R5), then unknown keys, then field errors in declaration order. Nested tables are checked when their field is converted, so errors of an outer table come before errors of an inner one. A misspelled required key is therefore reported as `unknown key App.prot`, which names the cause, rather than as `missing key App.port`.
- **U6 — One key.** If several keys are unknown, the error names the first in the iteration order of the `toml::Table`. That order is alphabetical by key unless the `preserve_order` feature of `toml` is enabled somewhere in the dependency graph, in which case it is the document order. This matches the other errors, which report the first problem found (RFC-0004 L5).
- **U7 — Unchanged behavior.** A failing `load` leaves the scope unchanged, as for every other load error. Saving is unchanged: `save` writes the known keys and replaces the section, as it does without the attribute. The store itself is never changed by a load, so the unknown keys stay in the store until a save of the scope or another store operation replaces the section.

## The error

- **U8 — New variant.** `Error` gains a variant, which is not a breaking change because `Error` is `#[non_exhaustive]`:

  ```rust
  /// A strict struct found a key that no loaded field maps.
  #[error("unknown key {section}.{key}")]
  UnknownKey {
      /// The section or nested table path, e.g. `App` or `App.upstream[1]`.
      section: String,
      /// The unknown key.
      key: String,
  },
  ```

  The constructor `Error::unknown_key(section, key)` is added next to `Error::missing_key`, so that manual implementations of `ConfigScope` and `ConfigTable` can report the same error.

## Compile-time diagnostics

| Input | Diagnostic |
|---|---|
| `#[confer(deny_unknown_keys)]` twice on one struct | `duplicate #[confer(deny_unknown_keys)] attribute` |
| `#[confer(deny_unknown_keys = true)]` or any other value | `#[confer(deny_unknown_keys)] takes no value` |
| `#[confer(deny_unknown_keys)]` on a field | `#[confer(deny_unknown_keys)] is a struct attribute` |

The existing message for unsupported struct attributes is unchanged.

## Implementation notes (non-normative)

- The derived code already moves each field's value out of an owned snapshot of the table (`table.remove(key)`). A strict struct calls a new `__private` helper before the field conversions, for example `__private::deny_unknown_keys(path, &table, &["host", "port"])?`, which returns the first key of the table not in the list. Checking before the conversions (U5) needs the list; checking the leftovers afterwards would need none, but would report conversion errors first (see [Resolved questions](#resolved-questions), Q2).
- A scope without loaded fields currently only checks that its section is a table and does not bind the snapshot. A strict scope without loaded fields binds it and rejects every key.
- The attribute is parsed in `parse_section_name`, which then returns a struct-level description instead of only the section name; `Scope` in `model.rs` gains a `deny_unknown_keys: bool`.

## Compatibility

- Additive. Existing code compiles and behaves as before; `Error::UnknownKey` is a new variant of a `#[non_exhaustive]` enum.
- Adding the attribute to an existing struct can make loading fail for configuration files that contain leftovers or keys of ignored fields. That is its purpose; the error names the key to remove.

## Alternatives considered

1. **Strict by default.** It would catch typos for everyone, but it is a breaking change for every application whose files contain leftovers, and it would reject files written by a newer version of the application. Opt-in first; the default can be reconsidered before 0.1.0.
2. **Inheritance into nested tables.** A strict scope would make its table types strict. The `ConfigTable::from_table(path, table)` signature has no way to pass the setting, so the trait would need a new parameter or method, and a table type would behave differently depending on where it is used. Per-type strictness keeps the trait unchanged and makes the behavior of a type visible on the type.
3. **Store-level strictness**, e.g. `ConfigStore::deny_unknown_keys()`. The store does not know which keys a scope maps, so it would need a registry of scopes. A derive attribute keeps the knowledge where it is.
4. **Warnings instead of errors**, e.g. through a callback or the `log` crate. The crate has no logging, and a warning is easy to miss; an application that wants to tolerate unknown keys keeps the default.
5. **Reporting all unknown keys at once**, e.g. `UnknownKeys { section, keys: Vec<String> }`. It saves a round trip when there are several typos, but differs from every other error, which reports one problem (see Q3).

## Out of scope

- Unknown top-level sections. A later store-level check could take a list of known sections, for example from the scopes registered by the application.
- Suggestions of similar keys in the error message.
- `#[confer(flatten)]` and other ideas that would change the set of known keys; they are listed in the [backlog](./BACKLOG.md).

## Testing

- Scope with the attribute: an unknown key, an unknown sub-table and an unknown array of tables fail in `from_store` and in `load` with `Error::UnknownKey` and the expected `section` and `key`; after a failing `load` the scope is unchanged.
- A key named like an ignored field and the Rust name of a renamed field are unknown (U2).
- A missing section and an absent table pass (U4); a strict scope without loaded fields rejects any key in its section.
- Precedence (U5): a section that is not a table is reported before unknown keys; an unknown key is reported before a `MissingKey` and before a `TypeMismatch` of a field; an unknown key in an outer table is reported before one in an inner table.
- Per-type behavior (U3): a lenient table type in a strict scope ignores unknown keys; a strict table type in a lenient scope rejects them, with paths such as `App.server` and `App.upstream[1]`.
- Several unknown keys: the error names the alphabetically first one (U6, with the default features of `toml`).
- Without the attribute, unknown keys are still ignored on load and removed on save (existing tests stay unchanged).
- `Error::UnknownKey` displays as `unknown key App.prot`; `Error::unknown_key` builds it.
- `trybuild`: the three diagnostics in [Compile-time diagnostics](#compile-time-diagnostics), and a struct deriving both macros with the attribute compiles.

## Documentation deliverables

- README (both copies): the struct attributes in [Derive attributes](../README.md#derive-attributes) gain `deny_unknown_keys`; [Saving configuration](../README.md#saving-configuration) mentions it next to the misspelled-key pitfall; Troubleshooting gets an entry for `unknown key`.
- Rustdoc: `Error::UnknownKey` and `Error::unknown_key`; the derive documentation in `conferencier-derive` and the crate-level attribute list.
- CHANGELOG: an *Added* entry.
- BACKLOG: the entry moves to *Done*; the list of planned work for 0.0.5 drops it.
- RFC-0004 stays unchanged: this RFC adds an opt-in mode and does not change O2 for structs without the attribute.

## Resolved questions

- **Q1 — Name of the attribute.** `deny_unknown_keys`. It resembles serde's widely used `deny_unknown_fields`, but uses the TOML term of the error message, because the check is about keys in the file, not Rust fields. The backlog's working name `deny_unknown` does not say what is denied. `strict` was rejected as ambiguous: the crate has other lenient conversions (integers for float fields, datetimes parsed from strings) that a user could expect a strict mode to disable, and the name would be taken if such a mode were ever wanted.
- **Q2 — Check before or after the conversions.** Before the conversions and every other check of the fields (U5): only the check that the table itself is a table comes first, because without a table there are no keys to check. A missing key, a type mismatch, a value out of range and errors inside nested tables are all reported after unknown keys, so that a misspelled required key reports the misspelling (`unknown key App.prot`) instead of the missing key (`missing key App.port`). Checking the leftovers after the conversions would need no key list in the generated code, but would report the consequence instead of the cause.
- **Q3 — One key or all.** One key (U6), consistent with the other errors, which report the first problem found, and with `Error`'s string fields. Reporting all unknown keys would need a different variant shape for little gain.
