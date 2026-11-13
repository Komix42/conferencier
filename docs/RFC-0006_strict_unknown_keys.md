# RFC-0006: Strict mode for unknown keys and unambiguous error paths

Date: 2026-11-12
Status: Draft
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.0.5
Amends: [RFC-0004](./RFC-0004_nested_tables.md)

## Document Scope

This RFC adds an opt-in struct attribute, `#[confer(deny_unknown_keys)]`, for `#[derive(ConfigScope)]` and `#[derive(ConfigTable)]`. A struct with the attribute rejects keys and sub-tables of its table that no loaded field maps, instead of ignoring them. It adds the error variant `Error::UnknownKey`. Structs without the attribute behave as before: unknown keys are ignored on load and removed on save ([RFC-0004](./RFC-0004_nested_tables.md) O2). Saving does not change.

It also makes the location in every key-related error unambiguous, so that `UnknownKey` and the existing variants report it the same way. The field `section` of `MissingKey`, `TypeMismatch` and `ValueParse` becomes `path`, holding the table's path in TOML dotted-key notation with every segment quoted when needed, including the section name; messages quote the key by the same rule; and an error about a top-level section reports the section as the key instead of the placeholder `<section>`. This part is a breaking change of `Error`.

It amends the following parts of [RFC-0004](./RFC-0004_nested_tables.md), whose text stays unchanged: E1 (the section name verbatim as the root of a path), E2 and E4 (the final key displayed without quoting, the path held in `section`), and the `<section>` key of R5 and of `set_section_table`.

The proposal starts from the [Strict mode for unknown keys](./BACKLOG.md#derive-attributes-and-field-types) entry of the backlog, first listed as future work in [RFC-0004](./RFC-0004_nested_tables.md#out-of-scope--future-work). It assumes the API redesign of [RFC-0005](./RFC-0005_api_redesign.md) (`ConfigScope`, `ConfigTable`, `ConfigStore`, `Error`) and the `default` expressions of [RFC-0007](./RFC-0007_default_expressions.md), which ship in the same release; the published 0.0.4 documentation still uses the old names. Decisions made while drafting and after review are recorded in [Resolved questions](#resolved-questions).

## Synopsis

```rust
use conferencier::{ConfigScope, ConfigStore, ConfigTable, Error};

#[derive(ConfigScope)]
#[confer(section = "App", deny_unknown_keys)]
struct AppConfig {
    #[confer(default = 8080)]
    port: u16,
    #[confer(table)]
    server: Option<Server>,
}

#[derive(ConfigTable)]
#[confer(deny_unknown_keys)]
struct Server {
    #[confer(default = "localhost".into())]
    host: String,
}

let store: ConfigStore = "[App]\nprot = 9090\n".parse()?;
match AppConfig::from_store(&store) {
    Err(Error::UnknownKey { path, key }) => assert_eq!((path.as_str(), key.as_str()), ("App", "prot")),
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
- [Error paths](#error-paths)
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
- One unambiguous way to report the location of a key in every error variant, including keys and section names that contain dots or spaces.
- No change for structs without the attribute, no change to saving and no change to the `ConfigScope` and `ConfigTable` traits.

Non-goals:

- Detecting unknown top-level sections. A store holds the sections of many scopes, and no single scope knows them all (see [Out of scope](#out-of-scope)).
- Suggestions for similar names ("did you mean `port`?").
- Strictness inherited from a scope by the table types it contains (see [Alternatives considered](#alternatives-considered), item 2).

## The attribute

- **U1 — Struct attribute.** `#[confer(deny_unknown_keys)]` is accepted on structs deriving `ConfigScope` or `ConfigTable`, next to `section`, in the same or a separate `#[confer(...)]` attribute. It takes no value. A struct deriving both macros applies it to both implementations.
- **U2 — Known keys.** The known keys of a struct are the keys of its loaded fields: scalar fields and `#[confer(table)]` fields, after `rename`. An ignored field has no key, so a key named like an ignored field is unknown. With `rename`, the Rust field name is not a known key.
- **U3 — Per type, not inherited.** The attribute applies only to the table of the struct that carries it. A table type nested in a strict scope stays lenient unless the table type has the attribute itself, and a strict table type is strict in every scope that uses it. Each element of an array of tables is checked by its element type. A strict scope with a lenient table type therefore still accepts `[App.server] prot = 1`; the documentation states this next to the attribute, and every table type that should catch typos carries the attribute itself.

## Semantics

- **U4 — Load.** `from_store`, `load` and `from_table` of a strict struct fail with `Error::UnknownKey` if the table contains a key that is not known (U2). Sub-tables, arrays of tables and inline tables are keys as well. A dotted key such as `server.host = "x"` defines the sub-table `server`, so only `server` is checked at this level. A missing section is an empty table and passes; a missing nested table is not checked, and its field follows the rules for missing keys ([RFC-0007](./RFC-0007_default_expressions.md) M1).
- **U5 — Precedence.** The check runs after the table itself has been obtained, and before any field is converted or checked: missing keys, type mismatches, out-of-range values and nested tables are all reported after unknown keys. For a scope, a section that is not a table is reported first ([RFC-0004](./RFC-0004_nested_tables.md) R5), then unknown keys, then field errors in declaration order. Nested tables are checked when their field is converted, so errors of an outer table come before errors of an inner one. A misspelled required key is therefore reported as `unknown key App.prot`, which names the cause, rather than as `missing key App.port`.
- **U6 — One key, deterministic.** If several keys are unknown, the error names the smallest of them in the ordering of Rust strings (byte-wise, which is alphabetical for ASCII keys). The choice does not depend on the iteration order of `toml::Table`, which is the document order if the `preserve_order` feature of `toml` is enabled anywhere in the dependency graph and alphabetical otherwise. Reporting one problem matches the other errors (RFC-0004 L5).
- **U7 — Unchanged behavior.** A failing `load` leaves the scope unchanged, as for every other load error. Saving is unchanged: `save` writes the known keys and replaces the section, as it does without the attribute. The store itself is never changed by a load, so the unknown keys stay in the store until a save of the scope or another store operation replaces the section.

## The error

- **U8 — New variant.** `Error` gains a variant (adding a variant to the `#[non_exhaustive]` enum is not breaking by itself):

  ```rust
  /// A strict struct found a key that no loaded field maps.
  #[error("unknown key {}", display_key(path, key))]
  UnknownKey {
      /// Path of the table containing the key, e.g. `App` or `App.upstream[1]` (P2).
      path: String,
      /// The unknown key, verbatim (P3).
      key: String,
  },
  ```

  The constructor `Error::unknown_key(path, key)` is added next to `Error::missing_key`, so that manual implementations of `ConfigScope` and `ConfigTable` can report the same error. Its fields follow [Error paths](#error-paths), like those of the other variants.

## Error paths

The rules apply to `MissingKey`, `TypeMismatch`, `ValueParse` and `UnknownKey`, and to every place that creates them: the store's methods, the derived code and the constructors.

- **P1 — `path` instead of `section`.** The field `section` of `MissingKey`, `TypeMismatch` and `ValueParse` is renamed `path`, and so is the first parameter of `Error::missing_key`, `Error::type_mismatch` and `Error::value_parse`. The field holds the path of the table that contains the key, which is a section only at the top level. `Error::Io { path }` keeps its name; it holds a file path and is a different variant.
- **P2 — TOML dotted-key notation.** `path` is written as a TOML dotted key: every segment, including the section name, is written bare if it is non-empty and consists only of `A-Z a-z 0-9 _ -`, and is quoted and escaped as a TOML basic string otherwise. An element of an array of tables appends `[index]` with a zero-based index (unchanged, RFC-0004 E3). A section named `App.server`, which is a single top-level key ([RFC-0004](./RFC-0004_nested_tables.md) O3), is therefore reported as `"App.server"`, and no longer looks like the nested table `server` of `App`. Before this RFC, the section name was used verbatim (E1) and only nested segments were quoted (E2).
- **P3 — The key verbatim, quoted in messages.** `key` holds the key exactly as in the document, so that it can be compared with a field's key. Messages display `path` and `key` joined by a dot, with the key quoted by the rule of P2: `missing key App."a.b"`, `unknown key App."my key"`. Before this RFC, the key was displayed without quoting (E2, E4), so `App.a.b` could mean the key `a.b` of `App` or the key `b` of `App.a`.
- **P4 — Top-level entries.** An error about a top-level entry itself has an empty `path` and the section name as `key`, and its message shows only the key. A section that is not a table is reported as `TypeMismatch { path: "", key: "App", expected: "table", found }`, displayed `expected table at App but found integer`. This replaces `key: "<section>"`, displayed `expected table at App.<section> but found integer`, in the store methods and in `load`, `from_store` and `save` ([RFC-0004](./RFC-0004_nested_tables.md) R5).
- **P5 — Who formats the path.** The store's methods and the derived code build paths by P2; `ConfigTable::from_table` and `to_table` receive the formatted path of their table, as before. The constructors take `path` as given. A manual implementation passes the path it received, or a section name, which is formatted correctly unless it needs quoting; public helpers for formatting paths are out of scope (see [Out of scope](#out-of-scope)).

Examples:

| Situation | `path` | `key` | Message |
|---|---|---|---|
| `port` missing in `[App]` | `App` | `port` | `missing key App.port` |
| `prot` unknown in `[App.server]` | `App.server` | `prot` | `unknown key App.server.prot` |
| `port` invalid in the second `[[App.upstream]]` | `App.upstream[1]` | `port` | `invalid value at App.upstream[1].port: ...` |
| `"a.b"` missing in `[App]` | `App` | `a.b` | `missing key App."a.b"` |
| `port` missing in `["my app"]` | `"my app"` | `port` | `missing key "my app".port` |
| `App = 1` read as a section | (empty) | `App` | `expected table at App but found integer` |

## Compile-time diagnostics

| Input | Diagnostic |
|---|---|
| `#[confer(deny_unknown_keys)]` twice on one struct | `duplicate #[confer(deny_unknown_keys)] attribute` |
| `#[confer(deny_unknown_keys = true)]` or any other value | `#[confer(deny_unknown_keys)] takes no value` |
| `#[confer(deny_unknown_keys)]` on a field | `#[confer(deny_unknown_keys)] is a struct attribute` |

The existing message for unsupported struct attributes is unchanged.

## Implementation notes (non-normative)

- The derived code already moves each field's value out of an owned snapshot of the table (`table.remove(key)`). A strict struct calls a new `__private` helper before the field conversions, for example `__private::deny_unknown_keys(path, &table, &["host", "port"])?`, which returns the smallest key of the table not in the list. Checking before the conversions (U5) needs the list; checking the leftovers afterwards would need none, but would report conversion errors first (see [Resolved questions](#resolved-questions), Q2).
- A scope without loaded fields currently only checks that its section is a table and does not bind the snapshot. A strict scope without loaded fields binds it and rejects every key.
- The attribute is parsed in `parse_section_name`, which then returns a struct-level description instead of only the section name; `Scope` in `model.rs` gains a `deny_unknown_keys: bool`.
- Error paths: `table_path.rs` gains a function that formats one segment by P2, used by `child_path` and for the root. The derive can format the section name when it expands, since it is a literal. The store formats its `section` argument in the errors it creates. The `#[error(...)]` messages call a private function that quotes the key.

## Compatibility

- **The attribute is additive.** Structs without it compile and behave as before.
- **Breaking: error fields.** Code that matches `Error::MissingKey { section, .. }`, `TypeMismatch { section, .. }` or `ValueParse { section, .. }` fails to compile and is changed to `path` (P1).
- **Breaking: error contents.** The following changes do not fail to compile, and change what a program sees at run time:
  - a section name that needs quoting appears quoted in `path` (P2);
  - a key that needs quoting appears quoted in messages, but not in `key` (P3);
  - a non-table section is reported with an empty `path` and the section as `key`, not with `key == "<section>"` (P4).

  Code comparing these strings, or the messages, has to be updated. Paths and messages remain diagnostic strings (RFC-0004 E5).
- Adding the attribute to an existing struct can make loading fail for configuration files that contain leftovers or keys of ignored fields. That is its purpose; the error names the key to remove.

## Alternatives considered

1. **Strict by default.** It would catch typos for everyone, but it is a breaking change for every application whose files contain leftovers, and it would reject files written by a newer version of the application. Opt-in first; the default can be reconsidered before 0.1.0.
2. **Inheritance into nested tables.** A strict scope would make its table types strict. The `ConfigTable::from_table(path, table)` signature has no way to pass the setting, so the trait would need a new parameter or method, and a table type would behave differently depending on where it is used. Per-type strictness keeps the trait unchanged and makes the behavior of a type visible on the type.
3. **Store-level strictness**, e.g. `ConfigStore::deny_unknown_keys()`. The store does not know which keys a scope maps, so it would need a registry of scopes. A derive attribute keeps the knowledge where it is.
4. **Warnings instead of errors**, e.g. through a callback or the `log` crate. The crate has no logging, and a warning is easy to miss; an application that wants to tolerate unknown keys keeps the default.
5. **Reporting all unknown keys at once**, e.g. `UnknownKeys { path, keys: Vec<String> }`. It saves a round trip when there are several typos, but differs from every other error, which reports one problem (see Q3).
6. **A structured path**, such as `Vec<PathSegment>`, instead of a string. It would make paths machine-readable without parsing, but adds a public type for a diagnostic need; P2 makes the string unambiguous, and apart from the `[index]` of array elements it is a valid TOML dotted key; a structured variant can be added later (see the backlog).
7. **`UnknownKey` following the old rules**, with the field `section`, and the other variants changed later. Every variant has the same ambiguity, and changing them separately would leave one release with two conventions.

## Out of scope

- Unknown top-level sections. A later store-level check could take a list of known sections, for example from the scopes registered by the application.
- Suggestions of similar keys in the error message.
- Public helpers that format paths by P2 for manual implementations, and a structured path (item 6); recorded in the [backlog](./BACKLOG.md#store-and-errors).
- `#[confer(flatten)]` and other ideas that would change the set of known keys; they are listed in the [backlog](./BACKLOG.md).

## Testing

- Scope with the attribute: an unknown key, an unknown sub-table and an unknown array of tables fail in `from_store` and in `load` with `Error::UnknownKey` and the expected `path` and `key`; after a failing `load` the scope is unchanged.
- A key named like an ignored field and the Rust name of a renamed field are unknown (U2).
- A missing section and an absent table pass (U4); a strict scope without loaded fields rejects any key in its section.
- Precedence (U5): a section that is not a table is reported before unknown keys; an unknown key is reported before a `MissingKey` and before a `TypeMismatch` of a field; an unknown key in an outer table is reported before one in an inner table.
- Per-type behavior (U3): a lenient table type in a strict scope ignores unknown keys; a strict table type in a lenient scope rejects them, with paths such as `App.server` and `App.upstream[1]`.
- Several unknown keys: the error names the smallest one (U6), also when it is not the first in the document.
- Inline tables and dotted keys (U4): an unknown key inside an inline table `server = { prot = 1 }` and inside a dotted key `server.prot = 1` is reported with the path `App.server`; an unknown dotted key `sever.host = "x"` is reported as the unknown key `sever`; a quoted key containing a dot, `"a.b" = 1`, is reported with `key == "a.b"` and the message `unknown key App."a.b"`.
- Without the attribute, unknown keys are still ignored on load and removed on save (existing tests stay unchanged).
- `Error::UnknownKey` displays as `unknown key App.prot`; `Error::unknown_key` builds it.
- Error paths, for every variant: each row of the examples in [Error paths](#error-paths), from the store's getters, from a derived scope and from a nested table type; a section named `App.server` gives `path == "\"App.server\""`; a key with a space, a quote or a control character is escaped in the message; a non-table section from the store methods that check a section (such as `set_section`, `ensure_section` and the typed setters), from `from_store`, `load` and `save` gives an empty `path` and the section as `key` (P4). Existing tests that match `section` or `<section>` are migrated.
- `trybuild`: the three diagnostics in [Compile-time diagnostics](#compile-time-diagnostics), and a struct deriving both macros with the attribute compiles.

## Documentation deliverables

- README (both copies): the struct attributes in [Derive attributes](../README.md#derive-attributes) gain `deny_unknown_keys`, with a prominent note that it is not inherited by nested table types (U3); [Saving configuration](../README.md#saving-configuration) mentions it next to the misspelled-key pitfall; Troubleshooting gets an entry for `unknown key`; every example and description that uses `section` of an error uses `path`, and the error descriptions state P2–P4.
- Rustdoc: `Error` with its variants and constructors, including the path rules and the examples table of [Error paths](#error-paths); `Error::UnknownKey` and `Error::unknown_key`; the derive documentation in `conferencier-derive` and the crate-level attribute list.
- CHANGELOG: an *Added* entry for the attribute, and **Breaking** entries under *Changed* for P1–P4.
- BACKLOG: the entry moves to *Done*; the list of planned work for 0.0.5 drops it.
- RFC-0004 gets an `Amended by: RFC-0006` header line when this RFC is implemented; its text stays unchanged. O2 does not change for structs without the attribute.

## Resolved questions

- **Q1 — Name of the attribute.** `deny_unknown_keys`. It resembles serde's widely used `deny_unknown_fields`, but uses the TOML term of the error message, because the check is about keys in the file, not Rust fields. The backlog's working name `deny_unknown` does not say what is denied. `strict` was rejected as ambiguous: the crate has other lenient conversions (integers for float fields, datetimes parsed from strings) that a user could expect a strict mode to disable, and the name would be taken if such a mode were ever wanted.
- **Q2 — Check before or after the conversions.** Before the conversions and every other check of the fields (U5): only the check that the table itself is a table comes first, because without a table there are no keys to check. A missing key, a type mismatch, a value out of range and errors inside nested tables are all reported after unknown keys, so that a misspelled required key reports the misspelling (`unknown key App.prot`) instead of the missing key (`missing key App.port`). Checking the leftovers after the conversions would need no key list in the generated code, but would report the consequence instead of the cause.
- **Q3 — One key or all.** One key (U6), consistent with the other errors, which report the first problem found, and with `Error`'s string fields. Reporting all unknown keys would need a different variant shape for little gain. Review asked for all keys to save fix-and-rerun rounds; kept at one, because several typos at once are rare and every other error has the same round trip.
- **Q4 — Which key.** After review, the smallest unknown key (U6) instead of the first in the iteration order of `toml::Table`, which depends on whether any crate in the dependency graph enables `toml/preserve_order`. Observable error behavior should not depend on feature unification.
- **Q5 — Field names and dotted keys.** Review proposed `path` instead of `section`, because the field holds nested paths such as `App.upstream[1]`, and quoting keys that contain dots in the message. Both concern every error variant, so this RFC changes all of them at once ([Error paths](#error-paths)) rather than giving `UnknownKey` a convention of its own (item 7). The section name is quoted as well (P2), because a section containing a dot was just as ambiguous, and the placeholder `<section>` is replaced by reporting the section as the key (P4), so that every location is one TOML dotted key.
- **Q6 — Inheritance and the default after review.** Review asked for inheritance into nested table types (U3) and for strict by default before 0.1.0. Inheritance needs a change of the `ConfigTable` trait and makes a type behave differently depending on where it is used (item 2); the documentation states the per-type rule prominently instead. Strict by default stays a decision before 0.1.0 (item 1); it would also be a natural point to add an opt-out such as `allow_unknown_keys`.
