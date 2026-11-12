# RFC-0007: Expressions in `default`

Date: 2026-11-12
Status: Draft
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.0.5
Amends: [RFC-0001](./RFC-0001_baseline_architecture.md), [RFC-0003](./RFC-0003_field_attributes.md)

## Document Scope

This RFC makes `#[confer(default = ...)]` take a plain Rust expression of the field's type, evaluated like the initializer of `#[confer(ignore = <expr>)]`. The literal forms of `default`, with their conversions (string literals to `String`, integer literals to floats, wrapping in `Some`, arrays to `Vec`, datetime strings checked at compile time), are removed. After this RFC, `default` and `ignore` follow one rule. This is a breaking change; every affected default fails to compile, so no behavior changes silently.

It amends the following parts of earlier RFCs, whose text stays unchanged (see [Documentation deliverables](#documentation-deliverables)):

- [RFC-0001](./RFC-0001_baseline_architecture.md): the definition of `default` in the [Glossary](./RFC-0001_baseline_architecture.md#glossary) as "a compile-time-checked Rust literal";
- [RFC-0003](./RFC-0003_field_attributes.md): [A3](./RFC-0003_field_attributes.md#field-attributes), which kept `default` a literal checked against the field type, and the out-of-scope note on the literal forms accepted by `default`.

The proposal starts from the [Expressions in `default`](./BACKLOG.md#derive-attributes-and-field-types) entry of the backlog. It uses the names from [RFC-0005](./RFC-0005_api_redesign.md). Decisions made while drafting are recorded in [Resolved questions](#resolved-questions).

## Synopsis

```rust
use conferencier::ConfigScope;

const DEFAULT_PORT: u16 = 8080;

fn default_workers() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

#[derive(ConfigScope)]
#[confer(section = "App")]
struct AppConfig {
    #[confer(default = DEFAULT_PORT)]
    port: u16,
    #[confer(default = String::from("localhost"))]
    host: String,
    #[confer(default = default_workers())]
    workers: usize,
    #[confer(default = vec![DEFAULT_PORT, 8081])]
    fallback_ports: Vec<u16>,
    #[confer(default = Some(1.0))]
    ratio: Option<f64>,
    #[confer(ignore = Vec::new())] // the same rule as `default`
    history: Vec<String>,
}
```

Before this RFC, `host` would be written `default = "localhost"`, `fallback_ports` could not use the constant, `ratio` would be `default = 1`, and `port` and `workers` could not be written at all.

## Table of Contents

- [Document Scope](#document-scope)
- [Synopsis](#synopsis)
- [Motivation](#motivation)
- [Goals and non-goals](#goals-and-non-goals)
- [The rule](#the-rule)
- [Evaluation](#evaluation)
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
- **Two rules for neighboring attributes.** Since [RFC-0003](./RFC-0003_field_attributes.md), `ignore = <expr>` takes any expression of the field's type, with no conversion. `default` takes only literals and converts them: `"text"` becomes a `String`, `1` becomes `1.0` for a float field, `5` becomes `Some(5)` for an `Option` field, `[1, 2]` becomes a `Vec`. A user has to learn which attribute converts what, and `ignore = "abc"` on a `String` field fails while `default = "abc"` works.
- **Timing.** At 0.0.x, breaking changes are expected. Every affected attribute fails to compile, so the migration cannot change behavior silently, unlike the default section name in RFC-0005 N7.

## Goals and non-goals

Goals:

- One rule for `default` and `ignore`: a Rust expression of the field's type, with no conversion.
- Any expression as a default: constants, function calls, blocks, macros.
- Type errors reported by the compiler at the expression.

Non-goals:

- Keeping the literal forms. They are removed (see [Alternatives considered](#alternatives-considered), item 1).
- Defaults for `#[confer(table)]` fields (listed under *More table shapes* in the [backlog](./BACKLOG.md#derive-attributes-and-field-types)).
- Defaults that depend on the store, on other fields or on the async context.
- Changes to loading from TOML. The lenient conversions of present values (an integer for a float field, a datetime parsed from a string) concern the content of the file, not attributes, and stay.

## The rule

- **D1 — An expression of the field's type.** `#[confer(default = <expr>)]` takes any Rust expression whose type is the field's type: `T` for a field of type `T`, `Option<T>` for `Option<T>`, `Vec<T>` for `Vec<T>`. The derive does not inspect the expression. A literal is an ordinary Rust literal: `"text"` is a `&str`, `1` is an integer, `[1, 2]` is an array.
- **D2 — Same as `ignore`.** The rule is the rule of `#[confer(ignore = <expr>)]` ([RFC-0003](./RFC-0003_field_attributes.md) A2), which does not change. Both attributes are documented together.
- **D3 — Unchanged restrictions.** `default` is accepted on loaded scalar fields only: combined with `ignore` or `table` it remains a compile error with the existing messages.

## Evaluation

- **D4 — Own closure.** The expression is evaluated in a closure that returns the field's type, as an `ignore` initializer is: `return` and `?` end the expression, not the generated function; `.await` is not possible; the expression cannot refer to the store, to other fields or to the locals and parameters of the generated code. It can use every item in scope at the struct, such as constants, statics and functions, and the struct's generic parameters.
- **D5 — When.** The expression is evaluated each time `from_store`, `load` or `from_table` finds the key missing, at the position of its field in declaration order. It is not evaluated when the key is present, even if the present value fails to convert, and it is never evaluated on save. If an earlier field fails, later expressions are not evaluated; if a later field fails, side effects of expressions that already ran are not undone, and the scope keeps its previous state as before. An `ignore` initializer of a scope still runs once, in `from_store`, after every loaded field has converted ([RFC-0003](./RFC-0003_field_attributes.md) A5).
- **D6 — Panics.** A panic in the expression propagates to the caller of `from_store`, `load` or `from_table`. No lock of the scope is held during the conversions, so the scope is left unchanged.

## Migration

Every literal default that relied on a conversion fails to compile and is rewritten as follows. Literals that already have the field's type, such as `default = 8080` on a `u16` field or `default = true` on a `bool` field, need no change.

| Field type | Before | After |
|---|---|---|
| `String` | `default = "localhost"` | `default = String::from("localhost")` or `default = "localhost".to_string()` |
| `f32`, `f64` | `default = 1` | `default = 1.0` |
| `Option<T>` | `default = 5` | `default = Some(5)` |
| `Vec<T>` | `default = [1, 2]` | `default = vec![1, 2]` |
| `Vec<String>` | `default = ["a", "b"]` | `default = vec!["a".to_string(), "b".to_string()]` |
| `Option<Vec<T>>` | `default = [1, 2]` | `default = Some(vec![1, 2])` |
| `Datetime` | `default = "2024-01-01T00:00:00Z"` | `default = "2024-01-01T00:00:00Z".parse().unwrap()` |
| `Option<Datetime>` | `default = "2024-01-01T00:00:00Z"` | `default = Some("2024-01-01T00:00:00Z".parse().unwrap())` |
| `Vec<Datetime>` | `default = ["2024-01-01T00:00:00Z"]` | `default = vec!["2024-01-01T00:00:00Z".parse().unwrap()]` |
| `Option<Vec<Datetime>>` | `default = ["2024-01-01T00:00:00Z"]` | `default = Some(vec!["2024-01-01T00:00:00Z".parse().unwrap()])` |

A datetime default is no longer validated at compile time. An invalid string panics in `from_store`, `load` or `from_table` when the key is missing (D6). A compile-time checked datetime macro is a possible later addition (see [Out of scope](#out-of-scope)).

## Compile-time diagnostics

| Input | Diagnostic |
|---|---|
| expression of the wrong type, e.g. `default = "localhost"` on a `String` field | the compiler's type error at the expression, e.g. `expected String, found &str`, usually with a suggestion such as `.to_string()` |
| `.await` in the expression | the compiler's error for `.await` outside an async context |
| a reference to `store` or another field | the compiler's error for an unknown name |
| `default` with `ignore` or `table` | unchanged |

The derive's own messages for literals are removed: `expected string literal` (and the other `expected ... literal` messages), `defaults for Vec<T> must use [ ... ] syntax`, `negative default for unsigned integer type` (now the compiler's error for negating an unsigned integer) and `invalid TOML datetime`.

## Implementation notes (non-normative)

- `default` reuses the code path of `ignore = <expr>`: the expression `e` becomes `(|| -> FieldType { e })()` in the absent-key branch generated by `absent_value` in `codegen.rs`. The closure's return type carries the span of the field type and the body the span of the expression, so type errors point at the expression.
- `transform_default`, `transform_plain_default`, `transform_option_default`, `transform_vec_default`, `validate_literal`, `strip_negation` and `literal_tokens` in `parser.rs` are removed. `conferencier-derive` then no longer uses `toml`, and the dependency is dropped, so the proc-macro crate no longer compiles `toml`.
- **Hygiene.** The `scope` parameter of the generated `load` and `save` is currently created with a call-site span, so an expression mentioning `scope` would capture the parameter instead of an item of the user's crate. It must use a mixed-site span, like `store`, `table` and `path` already do (`generated_local`).

## Compatibility

- **Breaking.** Defaults that relied on a conversion stop compiling (see [Migration](#migration)). Defaults whose literal already has the field's type compile unchanged with the same value.
- Nothing changes at run time for code that compiles, except that an invalid datetime string now panics on load instead of failing to compile.
- `ignore = <expr>` is unchanged.
- RFC-0006 is independent: a default never creates a key, so it does not interact with the check for unknown keys.

## Alternatives considered

1. **Literal shortcut and expressions side by side.** Literals keep their conversions, every other expression has the field's type. No existing default would break, and datetime strings would stay checked at compile time. But a user would have to know which syntax counts as a literal: `"localhost"` would work and `HOST` with `const HOST: &str` would not, `5` on an `Option` field would mean `Some(5)` and `MAX` would need `Some(MAX)`. Extending the shortcut to `ignore` would make the rules equal again, but keeps the conversions that make them hard to learn.
2. **Expressions with the value type.** An expression for an `Option<T>` field would have type `T` and be wrapped in `Some`, like the literal form; arrays would be converted element by element. It differs from `ignore`, where the expression has the field's type.
3. **Implicit conversion through `Into`.** `default = "localhost"` would work for a `String` field, but the conversion is invisible and breaks type inference: in `default = 8080 + 1` on a `u16` field, the literals fall back to `i32`, which has no `Into<u16>`.
4. **A separate attribute for functions**, such as serde's `default = "path::to::fn"`. A path in a string loses spans, like the removed `init = "..."` ([RFC-0003](./RFC-0003_field_attributes.md)), and a call expression covers the same case.
5. **Evaluating once and caching the value.** Generic structs cannot hold a per-type static, and a cached value hides changes, for example of the environment, from later loads.

## Out of scope

- A macro that parses a datetime at compile time, e.g. `conferencier::datetime!("2024-01-01T00:00:00Z")`, recorded in the [backlog](./BACKLOG.md#derive-attributes-and-field-types). Datetime defaults are expected to be rare.
- Defaults for table fields, and omitting values equal to their default on save.
- Expressions in other attributes, such as `section` or `rename`.

## Testing

- Expressions for a scope and a table type: a literal of the field's type, a constant, a function call, a method call, a block, `vec![...]`, `Some(...)`, an arithmetic expression whose literals take the field's type, and `.parse().unwrap()` for `Datetime`, wrapped in `Some(...)` and `vec![...]` for `Option<Datetime>`, `Vec<Datetime>` and `Option<Vec<Datetime>>`. Each yields the expected value when the key is missing and is not used when the key is present.
- Evaluation count (D5): a function that increments a static counter runs once per `from_store` or `load` with the key missing, not when the key is present, not when the present value fails to convert, not on save, and not when an earlier field fails.
- `return` and `?` inside a block expression end the expression (D4).
- A panicking expression leaves the scope unchanged after `load` (D6).
- Hygiene: an expression referring to constants named `store`, `scope`, `table`, `path` and `value` gets the user's constants, in `from_store`, `load` and `from_table`.
- Existing tests, examples and `trybuild` cases are migrated as in [Migration](#migration). `fail_invalid_datetime` and `fail_negative_unsigned` are removed, as their diagnostics no longer exist.
- `trybuild`: a string literal on a `String` field and an unwrapped value on an `Option` field (compiler errors at the expression), `.await` in the expression, and a reference to the `store` parameter. Compiler messages are pinned to the stable toolchain used by the existing `trybuild` tests.

## Documentation deliverables

- README (both copies): [Derive attributes](../README.md#derive-attributes) describes `default = <expr>` and `ignore = <expr>` with one rule and examples; the list of supported literal forms is removed; Troubleshooting replaces the entries for literal errors with the `&str` → `String` and `Some(...)` cases; every example uses the new form.
- Rustdoc: the derive documentation in `conferencier-derive` and the crate-level attribute list; examples use the new form.
- CHANGELOG: a **Breaking** entry under *Changed* with the table from [Migration](#migration), and an entry under *Removed* for the `toml` dependency of `conferencier-derive`.
- BACKLOG: the entry moves to *Done*; the list of planned work for 0.0.5 drops it; a new idea for a datetime macro.
- RFC-0001 and RFC-0003 get an `Amended by: RFC-0007` header line when this RFC is implemented; their text stays unchanged.

## Resolved questions

- **Q1 — Keep the literal forms?** No. `default` and `ignore` follow one rule, an expression of the field's type, with no hidden conversions. At 0.0.x a breaking change is acceptable, and every affected default fails to compile instead of changing behavior. Keeping the literal forms next to expressions was rejected as two rules within one attribute (see [Alternatives considered](#alternatives-considered), item 1).
- **Q2 — Type for `Option<T>` fields.** `Option<T>`, the field's type, as for `ignore`: `default = Some(5)`. Wrapping in `Some` would be a conversion.
- **Q3 — Exact type or conversion.** The exact type, with no `Into` (item 3). The compiler suggests `.to_string()` for the common `&str` case.
- **Q4 — Arrays.** An array is an ordinary Rust array, so a `Vec<T>` field takes `vec![...]` or any other expression of type `Vec<T>`.
- **Q5 — Datetime checks at compile time.** Given up; datetime defaults are expected to be rare. A datetime macro is left for later.
