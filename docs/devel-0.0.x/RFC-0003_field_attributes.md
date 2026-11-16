# RFC-0003: Field initialization: `ignore = <expr>` replaces `init`

Date: 2026-11-09
Status: Implemented
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.0.4
Amends: [RFC-0001](./RFC-0001_baseline_architecture.md)
Amended by: [RFC-0005](./RFC-0005_api_redesign.md), [RFC-0007](./RFC-0007_default_expressions.md)

## Document Scope

This RFC simplifies the field attributes and construction behavior of `#[derive(ConferModule)]`. The `init` attribute is removed, ignored fields get their initial value from `#[confer(ignore = <expr>)]`, and they no longer have a TOML key. `from_confer` constructs the module directly from successfully converted loaded values instead of first requiring a default value for every field. This is a breaking change of the derive macro's attribute syntax, but does not change TOML files, the `Confer` store or the `ConferModule` trait signatures.

It amends the descriptions of `init`, `ignore` and `default` in [RFC-0001](./RFC-0001_baseline_architecture.md): the [Glossary](./RFC-0001_baseline_architecture.md#glossary), the [derive crate summary](./RFC-0001_baseline_architecture.md#derive-crate-conferencier-derive), the [Contract of `from_confer`](./RFC-0001_baseline_architecture.md#contract-of-from_confer), the [attribute reference](./RFC-0001_baseline_architecture.md#conferencier-derive-procedural-macro), the example [Ignored fields and init expression](./RFC-0001_baseline_architecture.md#ignored-fields-and-init-expression), [Save semantics](./RFC-0001_baseline_architecture.md#interaction-with-derived-modules), [Edge cases and behavior](./RFC-0001_baseline_architecture.md#edge-cases-and-behavior) and the [compile-time contract tests](./RFC-0001_baseline_architecture.md#testing-strategy). RFC-0001 is a historical record: its text is not edited (see [Documentation deliverables](#documentation-deliverables)).

This RFC is self-contained for existing modules. It requires no new store API or persisted field shapes and can be implemented and released independently of later extensions.

## Synopsis

Before (0.0.3):

```rust
#[derive(ConferModule)]
struct Worker {
    #[confer(init = "Some(3)")] // never observable: `load` overwrites it
    retries: Option<u32>,
    #[confer(ignore, init = "Vec::with_capacity(16)")]
    buffer: Vec<String>,
    #[confer(ignore, default = 5)] // inserted as an unchecked expression
    attempts: u32,
}
```

After (0.0.4):

```rust
#[derive(ConferModule)]
struct Worker {
    #[confer(default = 3)] // used when the key is absent
    retries: Option<u32>,
    #[confer(ignore = Vec::with_capacity(16))]
    buffer: Vec<String>,
    #[confer(ignore = 5)]
    attempts: u32,
}
```

The `default` on `retries` in the new example intentionally changes the missing-key result from `None` to `Some(3)`. To preserve the old behavior of `init` on a loaded field, remove `init` without adding `default`; add `default` only when a fallback for an absent key is desired.

Direct construction also means a required loaded field no longer needs `Default`:

```rust
use conferencier::{ConferModule, Datetime};

#[derive(ConferModule)]
struct Clock {
    started_at: Datetime,
}
```

`Clock::from_confer` loads `started_at` directly; it returns `MissingKey` if the TOML key is absent.

## Table of Contents

- [Document Scope](#document-scope)
- [Synopsis](#synopsis)
- [Motivation](#motivation)
- [Field attributes](#field-attributes)
- [Migration](#migration)
- [Compile-time diagnostics](#compile-time-diagnostics)
- [Implementation notes (non-normative)](#implementation-notes-non-normative)
- [Compatibility](#compatibility)
- [Alternatives considered](#alternatives-considered)
- [Out of scope](#out-of-scope)
- [Milestones](#milestones)
- [Testing](#testing)
- [Documentation deliverables](#documentation-deliverables)

## Motivation

The derive should follow a simple mental model with two kinds of fields:

- **Loaded fields** (all fields not marked `ignore`) get their value from the TOML file. The only open question is what happens when the key is absent, and `default` answers it.
- **Ignored fields** belong to the configuration struct for application reasons (caches, handles, derived values), but the configuration file cannot affect them. The application may choose their initial value with `ignore = <expr>`; otherwise they start from `Default::default()`.

Each kind of field has exactly one attribute for its value. The model in 0.0.3 is more complex: `default` and `init` can be used with both kinds of fields, which gives eight combinations. Two of them are compile errors, one does not affect the resulting value and two mean almost the same thing. In detail:

- **`init` does not determine the value of a loaded field.** In 0.0.3 `from_confer` builds the struct from `init`, then immediately calls `load`, which overwrites every non-ignored field. A present key sets the value. For an absent key, an `Option` becomes `None`, and any other field uses `default` or fails with `MissingKey`. RFC-0001 ("unless `default` or `init` dictates otherwise") and both READMEs ("initializes a field before the first load") suggest otherwise. Apart from side effects of evaluating the expression, `init` has only one real effect on a loaded field: it supplies the pre-load value required by construction when the type has no `Default`. This was used as a workaround for a required `Datetime` field; direct construction removes the need for that workaround.
- **`default` has two meanings.** On a loaded field it is a literal, checked against the field type and used when the key is absent. On an ignored field it is inserted as an unchecked Rust expression and becomes the initial value.
- **Code in string literals loses spans.** `init = "Vec::new()"` is parsed from a string, so compiler errors in the expression point at the derive instead of at the expression.
- **Future extensions (informative).** A simpler attribute model also benefits planned extensions such as [RFC-0004](./RFC-0004_nested_tables.md). That proposal is not an implementation or release prerequisite for this RFC.

## Field attributes

- **A1 — `ignore` with an optional initializer.**
  - `#[confer(ignore)]` marks a field that the configuration file does not affect and initializes it with `Default::default()`.
  - `#[confer(ignore = <expr>)]` initializes it with the Rust expression `<expr>`, e.g. `#[confer(ignore = Vec::with_capacity(16))]`.
  - An ignored field has no TOML key. Combining `ignore` with `rename` is a compile error, and duplicate key detection skips ignored fields, so their names may equal the key of another field. This matches the RFC-0001 rule for [key collisions](./RFC-0001_baseline_architecture.md#conferencier-derive-procedural-macro), which covers non-ignored fields only; 0.0.3 also checked ignored fields.
  - The initial value, `<expr>` or `Default::default()`, is evaluated once, in `from_confer`, after all loaded fields have been converted successfully. Ignored fields are initialized in declaration order. Explicit `load` and `save` never evaluate the initial value or touch the field.
  - With an initializer, the field type does not need `Default`. In both forms, the field type does not have to be a type supported by the derive.
  - The initializer is a synchronous Rust expression evaluated in an isolated closure whose result is the field value. It cannot access the `store` argument of `from_confer`, other fields, or generated locals; `return` and `?` cannot escape the closure into the generated `from_confer` function. Whether `?` is valid inside the expression is otherwise ordinary Rust type checking for that closure. In particular, `.await` is not allowed. The expression can still use names available in the user's own scope.
- **A2 — Raw expressions only.** The initializer is written directly in the attribute. It keeps its source spans, so compiler errors point at it. A string literal is an ordinary `&str` expression, not code to be parsed. For example, `ignore = "abc"` on a `String` field is a type error; write `ignore = String::from("abc")` instead.
- **A3 — `default` applies to loaded fields only.** `default` keeps the RFC-0001 semantics: a literal, checked against the field type and used when the key is absent. Combining it with `ignore` is a compile error, because an ignored field has no key.
- **A4 — `init` is removed.** Any use of `init` is a compile error with a migration hint, not the generic "unsupported attribute" error.
- **A5 — `from_confer` directly constructs the module.** It first converts every loaded field in declaration order using the existing load rules and returns the first error, as `load` does. Then it constructs `Self` from those values and the initial values of the ignored fields. It does not create a pre-load `Self` or call `load` on a freshly constructed module. Consequently, loaded fields do not need `Default`; a required `Datetime` field without `default` is supported and still returns `MissingKey` if its key is absent. The `ConferModule` trait signatures and the behavior of explicit `load` and `save` are unchanged by this RFC.

| Attribute | Field | Initial value | `load` / `save` |
| --- | --- | --- | --- |
| `default = <literal>` | loaded | the stored value, or the literal when the key is absent | reads / writes the key |
| `ignore` | ignored | `Default::default()` | does not touch the field |
| `ignore = <expr>` | ignored | `<expr>` | does not touch the field |

Both forms of `ignore` are initialized once, in `from_confer`, after all loaded fields have converted successfully (A1, A5).

`section` is unchanged. `rename` is unchanged for loaded fields and rejected on ignored fields (A1).

## Migration

| 0.0.3 | 0.0.4 |
| --- | --- |
| `#[confer(ignore)]` | unchanged |
| `#[confer(ignore, init = "Vec::new()")]` | `#[confer(ignore = Vec::new())]`, or `#[confer(ignore)]` when the value equals `Default::default()` |
| `#[confer(ignore, default = 5)]` | `#[confer(ignore = 5)]` |
| `#[confer(ignore, rename = "...")]` | `#[confer(ignore)]`, because `rename` had no effect at runtime |
| `#[confer(init = ...)]` on a loaded field | Remove it; add `#[confer(default = ...)]` only if a fallback for an absent key is intended. |

For an ignored field, `init = <expr>` becomes `ignore = <expr>`; if the string form `init = "<expr>"` was used, drop the quotes. For a loaded field, remove `init`; add `default` only if a fallback on a missing key is intended.

A loaded field whose type lacks `Default` no longer needs a temporary initializer. Among the currently supported scalar and container types, only a required `Datetime` lacks `Default`; it can now be declared directly, without `default`, and remains required in the TOML file. If the key should instead be optional, use `#[confer(default = "...")]` or `Option<Datetime>`.

## Compile-time diagnostics

| Situation | Diagnostic |
| --- | --- |
| `init` in any form, alone or combined with other attributes (A4) | `#[confer(init)] was removed in 0.0.4; use #[confer(ignore = <expr>)] for ignored fields; remove it from loaded fields and add #[confer(default = ...)] only if a missing-key fallback is intended`, reported at `init`. |
| `ignore` combined with `default` (A3) | `#[confer(default)] cannot be combined with #[confer(ignore)]; use #[confer(ignore = <expr>)]`, reported at `default`. |
| `ignore` combined with `rename` (A1) | `#[confer(rename)] cannot be combined with #[confer(ignore)]; an ignored field has no TOML key`, reported at `rename`. |
| Duplicate `ignore`, including `ignore` together with `ignore = <expr>` | The existing `duplicate #[confer(ignore)] attribute` error. |
| `ignore =` not followed by a valid expression | A parse error at the attribute. |
| An initializer of the wrong type | An ordinary rustc type error pointing at the expression (A2). |

The existing error `#[confer(default = ...)] and #[confer(init = ...)] cannot be combined` is removed; that combination now gets the A4 error.

Messages are indicative; the exact wording is fixed by the `trybuild` snapshots.

## Implementation notes (non-normative)

- **Parser** (`conferencier-derive/src/parser.rs`):
  - `ignore` accepts an optional `= <expr>`, detected by peeking for `=`.
  - The `init` branch only reports A4.
  - `parse_init_expr` and the `default` + `init` check are removed.
  - `ignore` combined with `default` reports A3.
  - `ignore` combined with `rename` reports A1, and ignored fields are not registered for duplicate key detection.
- **Model** (`model.rs`): `init` is replaced by an optional initializer of ignored fields. `default` is kept only for loaded fields.
- **Codegen** (`codegen.rs`): `from_confer` constructs `Self` directly from converted field values instead of initializing every field and calling `load`.
  - An ignored field uses its initializer or `Default::default()`.
  - A loaded field is converted directly from the store using the existing conversion, default and missing-key behavior; it is not first initialized with `Default::default()`.
  - `from_confer` can obtain and convert loaded values through the existing typed getters, then construct `Self` without calling `load`. It does not require a whole-section snapshot or a new store helper. The implementation of explicit `load` and `save` is unchanged by this RFC.
  - All loaded fields are converted before ignored fields are initialized, so a failed conversion evaluates neither `ignore = <expr>` initializers nor `Default::default()` of bare `ignore` fields.
  - Emit an initializer inside a zero-argument closure and use hygienic spans for generated identifiers, so user expressions cannot capture generated locals and control flow stays inside the closure. The technique has to work for generic module structs.

## Compatibility

- **Breaking** change of the derive, listed as **Breaking:** in the changelog together with the migration table. Every use of `init` and of `ignore` combined with `default` or `rename` fails to compile with a hint. Code that still compiles loads the same values as before; the only runtime difference is when ignored fields are initialized (see below).
- For a loaded field, removing `init` preserves its old behavior: the key is loaded, `Option` becomes `None` when absent, and a required key without `default` still errors. Adding `default` deliberately changes the absent-key behavior.
- Ignored fields keep their initial values, but are now initialized only after all loaded fields convert successfully, and not at all when `from_confer` fails. This also applies to unchanged bare `#[confer(ignore)]` fields: side effects of an initializer or of a `Default` implementation can occur later than before, or not at all.
- Ignored fields no longer take part in duplicate key detection, so a struct previously rejected only for that reason now compiles (A1).
- Direct construction also makes loaded field types independent of `Default`; a required `Datetime` without `default` now compiles.
- TOML files, the `Confer` store, the `ConferModule` trait signatures, the error type and the MSRV are unaffected.

## Alternatives considered

1. **Keep `init` as a fallback for absent keys**, the behavior RFC-0001 and the README suggest. This would turn `init` into a second, expression-based `default` and keep two attributes with overlapping meaning. Expression-based defaults for loaded fields can be proposed separately.
2. **Allow `init` only together with `ignore`.** This removes the misleading case, but keeps two spellings for one thing (`ignore, init = ...` and `ignore, default = ...`) and the parsing of code from string literals.
3. **Deprecate `init` first and remove it later.** Stable procedural macros cannot emit warnings. The usual workaround, generating a use of a `#[deprecated]` item, produces confusing messages. For a 0.0.x release, a clear error with a migration hint is simpler.

## Out of scope

- Expression-based defaults for loaded fields.
- Changes to `section`, to `rename` on loaded fields and to the literal forms accepted by `default`.
- New persisted field shapes, including nested tables.
- Whole-section snapshot guarantees and changes to the atomicity of explicit `load` and `save`.

## Milestones

1. **Derive** — parser, model and codegen changes (A1–A5); migrated tests, example and `trybuild` cases; new tests.
2. **Documentation** — READMEs, changelog, cross-reference in RFC-0001 (see [Documentation deliverables](#documentation-deliverables)); RFC status → Implemented. An implementation report (like [IMPL-001](./IMPL_001_baseline.md)) is written only if the implementation has to depart from this RFC.

## Testing

- **Migrated:**
  - `conferencier/tests/module_integration.rs` and `conferencier/examples/advanced_usage.rs` move from `ignore, init = "..."` to `ignore = <expr>`.
  - The `trybuild` case `fail_conflicting_attrs` now expects the A4 error.
- **Integration tests:**
  - Values from `ignore` and `ignore = <expr>` are kept across `load` and are never written by `save`.
  - `ignore = <expr>` works on a user struct that implements neither `Default` nor any supported shape.
  - Neither an `ignore = <expr>` initializer nor the `Default` implementation of a bare `ignore` field is evaluated when conversion of a required loaded field fails.
  - A required `Datetime` field without `default` compiles, loads and saves; if its key is missing, `from_confer` returns `MissingKey`.
- **`trybuild`:**
  - Passing cases for a required `Datetime` field without `default`, and for `ignore = <expr>` on a type without `Default` or a supported loaded shape; include an ignored field named like the key of a loaded field (A1) and a generic module struct with an `ignore = <expr>` field.
  - Failing cases for every row of [Compile-time diagnostics](#compile-time-diagnostics), and initializers that refer to generated locals (e.g. `ignore = store.clone()`), use `.await`, or try to propagate an error out of `from_confer` with `?` (A1).
- **CI gates:** `cargo fmt`, `clippy -D warnings`, tests and `cargo doc` with `-D warnings`, as in the existing workflow.

## Documentation deliverables

- Root `README.md` and `conferencier/README.md` (runs as a doctest):
  - The attribute list split by the mental model from [Motivation](#motivation): fields loaded from TOML (`rename`, `default`) and fields the configuration file does not affect (`ignore`, `ignore = <expr>`).
  - `init` removed, `ignore` combined with `default` or `rename` rejected, plus a short migration note.
  - The known limitation about required `Datetime` fields removed.
  - In `conferencier/README.md`, replace the Troubleshooting entry "cannot combine default and init" with entries for the removed `init` and for `ignore` combined with `default` or `rename`.
- `conferencier-derive/README.md`: the attribute list in the reference sentence.
- `CHANGELOG.md` (Unreleased): a **Breaking:** entry with the migration hints and the changed initialization time of ignored fields; **Fixed:** entries for required `Datetime` fields without `default` and for duplicate key detection that no longer includes ignored fields.
- RFC-0001 stays unchanged as the historical record of the 0.0.1 baseline: an accepted RFC is amended by a later RFC, not edited. Only its status, cross-references between documents and typos may change. When this RFC is marked Implemented, RFC-0001 gets:
  - an `Amended by:` header line linking to this RFC;
  - a short note below the header saying that the document is a historical baseline, that a later RFC prevails where the two differ, and that the READMEs describe the current behavior.

  The amended sections are listed in [Document Scope](#document-scope) only; RFC-0001 gets no per-section markers.

  [IMPL-001](./IMPL_001_baseline.md) is a historical record and stays unchanged.
