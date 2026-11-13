# RFC-0007: Expressions in `default` and one rule for missing keys

Date: 2026-11-12
Status: Draft
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.0.5
Amends: [RFC-0001](./RFC-0001_baseline_architecture.md), [RFC-0003](./RFC-0003_field_attributes.md), [RFC-0004](./RFC-0004_nested_tables.md)

## Document Scope

This RFC makes `#[confer(default = ...)]` take a plain Rust expression of the field's type, evaluated like the initializer of `#[confer(ignore = <expr>)]`, and adds a bare `#[confer(default)]` that uses `Default::default()`, like a bare `#[confer(ignore)]`. The literal forms of `default`, with their conversions (string literals to `String`, integer literals to floats, wrapping in `Some`, arrays to `Vec`, datetime strings checked at compile time), are removed.

It also gives every loaded field, scalar or `#[confer(table)]`, one rule for a missing key:

> A missing key loads the field's `default` if it has one, `None` if the field is an `Option`, and is a `MissingKey` error otherwise.

`default` and `Option` exclude each other, so the rule has no overlapping cases. Table fields lose their implicit fallbacks (an empty table for `T`, an empty `Vec` for `Vec<T>`) and accept `default` instead, and an empty `Vec<T>` of tables is saved as `key = []`.

This is a breaking change. Every affected attribute fails to compile; the changed fallbacks of table fields fail loudly at load time with `MissingKey` naming the key, so no value changes silently.

It amends the following parts of earlier RFCs, whose text stays unchanged (see [Documentation deliverables](#documentation-deliverables)):

- [RFC-0001](./RFC-0001_baseline_architecture.md): the definition of `default` in the [Glossary](./RFC-0001_baseline_architecture.md#glossary) as "a compile-time-checked Rust literal";
- [RFC-0003](./RFC-0003_field_attributes.md): [A3](./RFC-0003_field_attributes.md#field-attributes), which kept `default` a literal checked against the field type, and the out-of-scope note on the literal forms accepted by `default`;
- [RFC-0004](./RFC-0004_nested_tables.md): the prohibition of `table` + `default` in [The `table` field attribute](./RFC-0004_nested_tables.md#the-table-field-attribute), the columns *Key absent on load* and *Save* of [Supported field shapes](./RFC-0004_nested_tables.md#supported-field-shapes) with their rationale, and alternative 7.

The proposal starts from the [Expressions in `default`](./BACKLOG.md#derive-attributes-and-field-types) entry of the backlog. It assumes the API redesign of [RFC-0005](./RFC-0005_api_redesign.md) (`ConfigScope`, `ConfigTable`, `ConfigStore`, `Error`), which ships in the same release; the published 0.0.4 documentation still uses the old names. Decisions made while drafting and after review are recorded in [Resolved questions](#resolved-questions).

## Synopsis

```rust
use conferencier::{ConfigScope, ConfigTable};

const DEFAULT_PORT: u16 = 8080;

fn default_workers() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

#[derive(ConfigScope)]
#[confer(section = "App")]
struct AppConfig {
    #[confer(default = DEFAULT_PORT)]
    port: u16,
    #[confer(default = "localhost".into())]
    host: String,
    #[confer(default = default_workers())]
    workers: usize,
    #[confer(default = vec![DEFAULT_PORT, 8081])]
    fallback_ports: Vec<u16>,
    #[confer(default)] // Vec::new()
    tags: Vec<String>,
    timeout: Option<u32>, // a missing key is None
    name: String,         // required: a missing key is MissingKey
    #[confer(table, default)] // Tls::default()
    tls: Tls,
    #[confer(table, default)] // an empty Vec
    upstream: Vec<Upstream>,
    #[confer(ignore = Vec::new())] // the same rule as `default`
    history: Vec<String>,
}

#[derive(ConfigTable, Default)]
struct Tls {
    #[confer(default)]
    enabled: bool,
    cert: Option<String>,
}

#[derive(ConfigTable)]
struct Upstream {
    url: String,
}
```

Before this RFC, `host` would be written `default = "localhost"`, `fallback_ports` could not use the constant, `port` and `workers` could not be written at all, `tags` needed `default = []`, and `tls` and `upstream` could not have an attribute: a missing `[App.tls]` loaded from an empty table, a missing `upstream` as an empty `Vec`.

## Table of Contents

- [Document Scope](#document-scope)
- [Synopsis](#synopsis)
- [Motivation](#motivation)
- [Goals and non-goals](#goals-and-non-goals)
- [The `default` attribute](#the-default-attribute)
- [Missing keys](#missing-keys)
- [Evaluation](#evaluation)
- [Saving](#saving)
- [Migration](#migration)
- [Compile-time diagnostics](#compile-time-diagnostics)
- [Implementation notes (non-normative)](#implementation-notes-non-normative)
- [Compatibility](#compatibility)
- [Alternatives considered](#alternatives-considered)
- [Out of scope](#out-of-scope)
- [Testing](#testing)
- [Documentation deliverables](#documentation-deliverables)
- [Resolved questions](#resolved-questions)

## Motivation

- **Defaults are duplicated.** A default that is also used elsewhere, such as a port in a constant shared with tests or a command-line parser, has to be repeated as a literal.
- **Computed defaults are impossible.** A default that depends on the machine, such as the number of CPUs, cannot be written; the field has to become an `Option` and the application applies the fallback after every load.
- **Two rules for neighboring attributes.** Since [RFC-0003](./RFC-0003_field_attributes.md), `ignore = <expr>` takes any expression of the field's type, with no conversion, and a bare `ignore` uses `Default::default()`. `default` takes only literals and converts them: `"text"` becomes a `String`, `1` becomes `1.0` for a float field, `5` becomes `Some(5)` for an `Option` field, `[1, 2]` becomes a `Vec`. A user has to learn which attribute converts what, and `ignore = "abc"` on a `String` field fails while `default = "abc"` works.
- **Different fallbacks per field kind.** A missing scalar key without `default` is an error, a missing table loads from an empty table, a missing array of tables is an empty `Vec`. A table field cannot say otherwise, because `table` + `default` is rejected. A user has to know which kind of field falls back to what.
- **Timing.** At 0.0.x, breaking changes are expected. Every affected attribute fails to compile and every changed fallback fails with an error naming the key, so the migration cannot change behavior silently, unlike the default section name in RFC-0005 N7.

## Goals and non-goals

Goals:

- One rule for `default` and `ignore`: a Rust expression of the field's type, with no conversion, or `Default::default()` when the attribute has no value.
- Any expression as a default: constants, function calls, blocks, macros.
- One rule for missing keys, the same for scalar and table fields.
- Type errors reported by the compiler at the expression.

Non-goals:

- Keeping the literal forms. They are removed (see [Alternatives considered](#alternatives-considered), item 1).
- Defaults that depend on the store, on other fields or on the async context.
- Changes to loading present values from TOML. The lenient conversions of present values (an integer for a float field, a datetime parsed from a string) concern the content of the file, not attributes, and stay.
- Generating `Default` implementations from `confer` attributes (see [Out of scope](#out-of-scope)).

## The `default` attribute

- **D1 — An expression of the field's type.** `#[confer(default = <expr>)]` takes any Rust expression whose type is the field's type: `T` for a field of type `T`, `Vec<T>` for `Vec<T>`. The derive does not inspect the expression. A literal is an ordinary Rust literal: `"text"` is a `&str`, `1` is an integer, `[1, 2]` is an array.
- **D2 — Bare `default`.** `#[confer(default)]` is the same as `#[confer(default = Default::default())]`. The field type must implement `Default`.
- **D3 — Same as `ignore`.** The rules of D1 and D2 are the rules of `#[confer(ignore = <expr>)]` and `#[confer(ignore)]` ([RFC-0003](./RFC-0003_field_attributes.md) A2), which do not change. Both attributes are documented together.
- **D4 — Where.** `default` is accepted on every loaded field, scalar or `#[confer(table)]`, except on `Option` fields (M2). Combined with `ignore` it remains a compile error with the existing message; given twice, in any form, it is a duplicate attribute.

## Missing keys

- **M1 — One rule.** When `from_store`, `load` or `from_table` finds the key of a loaded field missing, the field gets:
  1. the value of its `default` (D1, D2), if it has one;
  2. `None`, if its type is `Option<_>`;
  3. otherwise the load fails with `Error::MissingKey` naming the key.

  The rule is the same for scalar fields and for `#[confer(table)]` fields of every shape (`T`, `Option<T>`, `Vec<T>`). A present key is converted from TOML; its `default` is not used, even if the conversion fails. An ignored field has no key and is not covered by this rule; it is initialized from its `ignore` value.
- **M2 — `Option` excludes `default`.** `default` on a field whose type is `Option<_>` is a compile error. An `Option` field already states that its key may be missing, and a `default` would make `None` impossible to keep: `None` is saved by omitting the key (S1), and the next load would yield the default again. A field that should have a value when the key is missing has the non-`Option` type with `default`. `ignore` on an `Option` field is unchanged.
- **M3 — Table fields.** A missing table `T` without `default` is a `MissingKey`, no longer loaded from an empty table, and a missing array of tables `Vec<T>` without `default` is a `MissingKey`, no longer an empty `Vec`. A missing `Option<T>` is `None`, as before. `#[confer(table, default)]` uses `T::default()` or `Vec::new()`, and `#[confer(table, default = <expr>)]` any expression of the field's type.
- **M4 — `Default` of table types.** A bare `default` on a table field calls the `Default` implementation of the field type. It is plain Rust: the standard `#[derive(Default)]` does not know `confer` attributes, so a table type with `#[confer(default = 8080)] port: u16` and a derived `Default` has `port == 0` in `T::default()`. A type whose `Default` should match its `confer` defaults implements `Default` by hand, or the field uses `default = <expr>`.

## Evaluation

- **D5 — Own closure.** The expression is evaluated in a closure that returns the field's type, as an `ignore` initializer is: `return` and `?` end the expression, not the generated function; `.await` is not possible; the expression cannot refer to the store, to other fields or to the locals and parameters of the generated code. It can use every item in scope at the struct, such as constants, statics and functions, and the struct's generic parameters.
- **D6 — When.** The expression is evaluated each time `from_store`, `load` or `from_table` finds the key missing, at the position of its field in declaration order. It is not evaluated when the key is present, even if the present value fails to convert, and it is never evaluated on save. If an earlier field fails, later expressions are not evaluated; if a later field fails, side effects of expressions that already ran are not undone, and the scope keeps its previous state as before. An `ignore` initializer of a scope still runs once, in `from_store`, after every loaded field has converted ([RFC-0003](./RFC-0003_field_attributes.md) A5).
- **D7 — Panics.** A panic in the expression propagates to the caller of `from_store`, `load` or `from_table`. No lock of the scope is held during the conversions, so the scope is left unchanged.

## Saving

- **S1 — Every value is written, except `None`.** `save` writes every loaded field. `None` omits the key, because TOML has no null; by M1 a missing key loads as `None` again. An empty `Vec<T>` of tables is written as `key = []`, like an empty scalar `Vec`, instead of being omitted; otherwise a field with a non-empty `default`, or without `default`, would not survive a save and the next load. A file with `upstream = []` cannot get `[[App.upstream]]` entries added by hand, because TOML forbids extending a static array; the entry is removed or replaced first.
- **S2 — Defaults are saved like other values.** A default becomes an ordinary value of the field, so the next `save` writes it into the file. A computed default, such as `default_workers()`, is then frozen into the file and no longer recomputed. A value that should be computed at every start unless it is configured belongs in an `Option` field, with the fallback applied by the application. Omitting values equal to their default on save is out of scope.

## Migration

Every literal default that relied on a conversion, and every `default` on an `Option` field, fails to compile and is rewritten as follows. Literals that already have the field's type, such as `default = 8080` on a `u16` field or `default = true` on a `bool` field, need no change. `default = 1` on a float field does not compile: an integer literal is never inferred as a float, so `|| -> f64 { 1 }` fails with `E0308`.

| Field type | Before | After |
|---|---|---|
| `String` | `default = "localhost"` | `default = "localhost".into()` (or `.to_owned()`, `String::from(...)`) |
| `f32`, `f64` | `default = 1` | `default = 1.0` |
| `Vec<T>` | `default = [1, 2]` | `default = vec![1, 2]` |
| `Vec<T>` | `default = []` | `default` |
| `Vec<String>` | `default = ["a", "b"]` | `default = vec!["a".into(), "b".into()]` |
| `Datetime` | `default = "2024-01-01T00:00:00Z"` | `default = "2024-01-01T00:00:00Z".parse().unwrap()` |
| `Vec<Datetime>` | `default = ["2024-01-01T00:00:00Z"]` | `default = vec!["2024-01-01T00:00:00Z".parse().unwrap()]` |
| `Option<T>` | `default = 5` | field type `T`, `default = 5` |
| `Option<Vec<T>>` | `default = [1, 2]` | field type `Vec<T>`, `default = vec![1, 2]` |
| `Option<Datetime>` | `default = "2024-01-01T00:00:00Z"` | field type `Datetime`, `default = "2024-01-01T00:00:00Z".parse().unwrap()` |

The `Option` rows change only the type: before this RFC, such a field loaded `Some(default)` whenever the key was missing, so a `None` could not survive a save and a load either (M2).

The following changes do not fail to compile. A configuration file without the key fails to load with `MissingKey` naming the key, until the field gets `default`:

| Table field | Before, key missing | After, key missing | To keep the old behavior |
|---|---|---|---|
| `#[confer(table)] server: T` | `T` loaded from an empty table | `MissingKey` | `#[confer(table, default)]` with an `impl Default for T` matching its `confer` defaults, or `default = <expr>` (M4) |
| `#[confer(table)] upstream: Vec<T>` | empty `Vec` | `MissingKey` | `#[confer(table, default)]` |
| `#[confer(table)] server: Option<T>` | `None` | `None` | no change |

An empty `Vec<T>` of tables is now saved as `upstream = []` (S1). Files saved before keep loading; a missing key needs `default` as above.

A datetime default is no longer validated at compile time. An invalid string panics in `from_store`, `load` or `from_table`, and only when the key is missing (D7), so tests should load a configuration without the key. A compile-time checked datetime macro is a possible later addition (see [Out of scope](#out-of-scope)).

## Compile-time diagnostics

| Input | Diagnostic |
|---|---|
| expression of the wrong type, e.g. `default = "localhost"` on a `String` field | the compiler's type error at the expression, e.g. `expected String, found &str`, usually with a suggestion such as `.to_string()` |
| bare `default` on a type without `Default` | the compiler's error for the unsatisfied `Default` bound, pointing at the attribute |
| `.await` in the expression | the compiler's error for `.await` outside an async context |
| a reference to `store` or another field | the compiler's error for an unknown name |
| `default` on an `Option` field | `#[confer(default)] cannot be used on an Option field: a missing key already loads as None` with the help `use the inner type with #[confer(default = ...)], or remove the attribute` |
| `default` with `ignore` | unchanged |
| `default` twice, with or without a value | `duplicate #[confer(default)] attribute` (was `duplicate #[confer(default = ...)] attribute`) |

`Option` is recognized syntactically, as the other field shapes are ([RFC-0004](./RFC-0004_nested_tables.md#supported-field-shapes)): a type alias of an `Option` is treated as a plain type, so its missing key is a `MissingKey` unless it has `default`.

The derive's own messages for literals are removed: `expected string literal` (and the other `expected ... literal` messages), `defaults for Vec<T> must use [ ... ] syntax`, `negative default for unsigned integer type` (now the compiler's error for negating an unsigned integer) and `invalid TOML datetime`. The message `#[confer(table)] cannot be combined with #[confer(default)]` is removed; the one for `ignore` stays.

## Implementation notes (non-normative)

- `default` reuses the code path of `ignore = <expr>`: the expression `e` becomes `(|| -> FieldType { e })()` in the absent-key branch, and a bare `default` becomes `<FieldType as Default>::default()` with the span of the attribute. The closure's return type carries the span of the field type and the body the span of the expression, so type errors point at the expression.
- `absent_value` in `codegen.rs` implements M1 for every field: the default if present, `None` for `Option` shapes, `MissingKey` otherwise. `table_from_value` loses its own absent-key fallbacks (an empty table for `T`, `Vec::new()` for `Vec<T>`) and uses `absent_value`. `table_insert` writes an empty `Vec<T>` as an empty array.
- `transform_default`, `transform_plain_default`, `transform_option_default`, `transform_vec_default`, `validate_literal`, `strip_negation` and `literal_tokens` in `parser.rs` are removed. `conferencier-derive` then no longer uses `toml`, and the dependency is dropped, so the proc-macro crate no longer compiles `toml`. `parse_field` drops the `table` + `default` error and adds the `Option` + `default` error.
- **Hygiene.** The `scope` parameter of the generated `load` and `save` is currently created with a call-site span, so an expression mentioning `scope` would capture the parameter instead of an item of the user's crate. It must use a mixed-site span, like `store`, `table` and `path` already do (`generated_local`).

## Compatibility

- **Breaking at compile time.** Defaults that relied on a conversion and defaults on `Option` fields stop compiling (see [Migration](#migration)). Defaults whose literal already has the field's type compile unchanged with the same value.
- **Breaking at load time.** A missing table `T` or array of tables `Vec<T>` without `default` fails with `MissingKey` instead of falling back. The error names the key, and the fix is one attribute.
- **Saved files.** An empty `Vec<T>` of tables is written as `key = []` instead of being omitted. Files written before still load if the field has `default`.
- An invalid datetime string in a default panics on load instead of failing to compile.
- `ignore` and `ignore = <expr>` are unchanged.
- [RFC-0006](./RFC-0006_strict_unknown_keys.md) is independent: a default never creates a key, and a missing key is not an unknown key.

## Alternatives considered

1. **Literal shortcut and expressions side by side.** Literals keep their conversions, every other expression has the field's type. No existing default would break, and datetime strings would stay checked at compile time. But a user would have to know which syntax counts as a literal: `"localhost"` would work and `HOST` with `const HOST: &str` would not, `5` on an `Option` field would mean `Some(5)` and `MAX` would need `Some(MAX)`. Extending the shortcut to `ignore` would make the rules equal again, but keeps the conversions that make them hard to learn.
2. **Expressions with the value type.** An expression for an `Option<T>` field would have type `T` and be wrapped in `Some`, like the literal form; arrays would be converted element by element. It differs from `ignore`, where the expression has the field's type.
3. **Implicit conversion through `Into`.** `default = "localhost"` would work for a `String` field, but the conversion is invisible and breaks type inference: in `default = 8080 + 1` on a `u16` field, the literals fall back to `i32`, which has no `Into<u16>`. Writing `.into()` explicitly keeps the conversion visible and lets the target type drive inference.
4. **A separate attribute for functions**, such as serde's `default = "path::to::fn"`. A path in a string loses spans, like the removed `init = "..."` ([RFC-0003](./RFC-0003_field_attributes.md)), and a call expression covers the same case.
5. **Evaluating once and caching the value.** Generic structs cannot hold a per-type static, and a cached value hides changes, for example of the environment, from later loads.
6. **`default = Some(...)` on `Option` fields.** It would keep the literal form's use case, but `None` could not survive a save and a load (M2), and the field would behave like a non-`Option` field with a `None` that disappears.
7. **`Option` fields only with an explicit `default`.** Every field whose key may be missing would carry `default`, and a missing key of an `Option` field without it would be an error. It removes the `Option` case from M1, but contradicts what the type says: an `Option` field is expected to accept a missing key, and `None` survives a save by omitting the key.
8. **Keeping the implicit fallbacks of table fields** ([RFC-0004](./RFC-0004_nested_tables.md)). A missing table loads from an empty table, so adding a group with defaulted fields does not break existing files without any attribute. It keeps a fallback that applies to table fields only and cannot be turned off, while a scalar field without `default` is required.
9. **Bare `default` on a table `T` meaning "load from an empty table"** instead of `T::default()`. It would avoid writing `Default` by hand (M4), but a bare `default` would mean different things on different fields, and `ignore` on the same type would use `T::default()`. Generating a `Default` from the `confer` attributes is a separate, later feature (see [Out of scope](#out-of-scope)).
10. **Omitting an empty `Vec<T>` of tables on save** ([RFC-0004](./RFC-0004_nested_tables.md)). It keeps `[[...]]` entries addable by hand, but with M1 an omitted key loads the field's `default` or fails, so an empty `Vec` would not survive a save and a load.

## Out of scope

- A macro that parses a datetime at compile time, e.g. `conferencier::datetime!("2024-01-01T00:00:00Z")`, recorded in the [backlog](./BACKLOG.md#derive-attributes-and-field-types). It can be added later without breaking changes.
- Generating a `Default` implementation from the `confer` attributes of a struct (M4), recorded in the backlog.
- Omitting values equal to their default on save.
- `Option<Vec<T>>` table fields (still postponed by [RFC-0004](./RFC-0004_nested_tables.md)).
- Expressions in other attributes, such as `section` or `rename`.

## Testing

- Expressions for a scope and a table type: a literal of the field's type, a constant, a function call, a method call, `.into()`, a block, `vec![...]`, an arithmetic expression whose literals take the field's type, and `.parse().unwrap()` for `Datetime` and `Vec<Datetime>`. Each yields the expected value when the key is missing and is not used when the key is present.
- Bare `default` on a scalar, a `Vec<T>`, a table `T` with a hand-written `Default` and a table `Vec<T>`.
- Missing keys (M1): a required scalar, `Vec<T>`, table `T` and table `Vec<T>` without `default` fail with `MissingKey` and the expected path, also nested inside a table type; `Option<T>`, `Option<Vec<T>>` and table `Option<T>` load as `None`.
- Saving (S1): `None` omits the key; an empty table `Vec<T>` is written as `key = []` and loads back as empty, also when the field's `default` is non-empty; a computed default is written by `save` (S2).
- Evaluation count (D6): a function that increments a static counter runs once per `from_store` or `load` with the key missing, not when the key is present, not when the present value fails to convert, not on save, and not when an earlier field fails.
- `return` and `?` inside a block expression end the expression (D5).
- A panicking expression leaves the scope unchanged after `load` (D7).
- Hygiene: an expression referring to constants named `store`, `scope`, `table`, `path` and `value` gets the user's constants, in `from_store`, `load` and `from_table`.
- Existing tests, examples and `trybuild` cases are migrated as in [Migration](#migration). Tests of the removed table fallbacks are changed to the new behavior. `fail_invalid_datetime`, `fail_negative_unsigned` and the `table` + `default` case of `fail_table_conflicts` are removed, as their diagnostics no longer exist.
- `trybuild`: a string literal on a `String` field, an integer literal on an `f64` field, bare `default` on a type without `Default`, `default` on an `Option` field and on an `Option` table field, `.await` in the expression, and a reference to the `store` parameter. Compiler messages are pinned to the stable toolchain used by the existing `trybuild` tests.

## Documentation deliverables

- README (both copies):
  - [Derive attributes](../README.md#derive-attributes) describes `default`, `default = <expr>`, `ignore` and `ignore = <expr>` with one rule and examples, states M1 in one sentence, and explains the `Default` of table types (M4); the list of supported literal forms is removed.
  - The section on nested tables replaces the implicit fallbacks of table fields by M1 and states that an empty `Vec<T>` is saved as `key = []`, which blocks adding `[[...]]` entries by hand.
  - [Saving configuration](../README.md#saving-configuration) mentions that defaults are saved (S2).
  - Troubleshooting replaces the entries for literal errors with the `&str` → `String` case, `default` on an `Option` field and a `MissingKey` for a table that used to fall back.
  - Every example uses the new form.
- Rustdoc: the derive documentation in `conferencier-derive` and the crate-level attribute list; examples use the new form.
- CHANGELOG: **Breaking** entries under *Changed* with the tables from [Migration](#migration), including the shortcuts `.into()` and `vec!["a".into(), ...]`, and an entry under *Removed* for the `toml` dependency of `conferencier-derive`.
- BACKLOG: the entry moves to *Done*; the list of planned work for 0.0.5 drops it; new ideas for a datetime macro and for a generated `Default`.
- RFC-0001, RFC-0003 and RFC-0004 get an `Amended by: RFC-0007` header line when this RFC is implemented; their text stays unchanged.

## Resolved questions

- **Q1 — Keep the literal forms?** No. `default` and `ignore` follow one rule, an expression of the field's type, with no hidden conversions. At 0.0.x a breaking change is acceptable, and every affected default fails to compile instead of changing behavior. Keeping the literal forms next to expressions was rejected as two rules within one attribute (see [Alternatives considered](#alternatives-considered), item 1).
- **Q2 — Type of the expression.** The field's type, as for `ignore`. Wrapping in `Some` would be a conversion; with M2 the question does not arise for `Option` fields.
- **Q3 — Exact type or conversion.** The exact type, with no `Into` (item 3). The compiler suggests `.to_string()` for the common `&str` case, and an explicit `.into()` is as short as a hidden conversion. An integer literal for a float field does not compile; the claim from review that `|| -> f64 { 1 }` infers `1` as `f64` was checked and is wrong (`E0308`).
- **Q4 — Arrays.** An array is an ordinary Rust array, so a `Vec<T>` field takes `vec![...]` or any other expression of type `Vec<T>`.
- **Q5 — Datetime checks at compile time.** Given up for 0.0.5; datetime defaults are expected to be rare. Review pointed out that an invalid string then panics, and only when the key is missing; the documentation says so. A datetime macro is left for later and can be added without breaking changes.
- **Q6 — Bare `default`.** Accepted after review (D2), mirroring a bare `ignore`. It replaces `default = String::new()`, `default = Vec::new()` and the like, and gives table fields a short way to accept a missing key.
- **Q7 — `Option` fields.** A missing key loads as `None` without an attribute, and `default` on an `Option` field is rejected (M2). Requiring `default` on every field whose key may be missing (item 7) was considered for a rule without the `Option` case, and rejected because the type already says it. Allowing `default = Some(...)` (item 6) was rejected because `None` would not survive a save.
- **Q8 — Table fields.** Under M1 like every other field (M3). The implicit fallbacks of [RFC-0004](./RFC-0004_nested_tables.md) are replaced by `#[confer(table, default)]`, which the same RFC rejected only because the fallbacks existed.
- **Q9 — Empty `Vec<T>` of tables on save.** Written as `key = []` (S1), like an empty scalar `Vec`. Omitting it would break the roundtrip under M1 (item 10); the cost is that `[[...]]` entries cannot be added by hand to such a file.
- **Q10 — Generated `Default`.** Not in 0.0.5 (M4). A bare `default` on a table field uses the type's `Default` as plain Rust does; a `Default` that honors the `confer` defaults is a later, opt-in feature.
