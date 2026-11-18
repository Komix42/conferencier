# RFC-0011: Public API polish before 0.1.0

Date: 2026-11-17
Status: Draft
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.1.0-beta.1
Amends: [RFC-0001](../devel-0.0.x/RFC-0001_baseline_architecture.md)
Related: [AUDIT-001](../devel-0.0.x/AUDIT-001_quality_0.0.8.md), [RFC-0006](../devel-0.0.x/RFC-0006_strict_unknown_keys.md), [RFC-0007](../devel-0.0.x/RFC-0007_default_expressions.md)

## Document Scope

This RFC specifies the public behavior changes that resolve findings A1–A7 of the
[quality audit of 0.0.8](../devel-0.0.x/AUDIT-001_quality_0.0.8.md), as planned in
the [backlog](../BACKLOG.md#planned-for-010-beta1). It covers error messages,
sources and extensibility, file paths in parse errors, the Tokio features enabled
by the library, non-finite `f32` values, the signatures of the typed setters and
the `Debug` output of `ConfigStore`. It also records the decision about strict
unknown-key handling by default, which
[RFC-0006](../devel-0.0.x/RFC-0006_strict_unknown_keys.md) Q6 postponed until
before 0.1.0.

Internal fixes from the audit (A8, B1–B5) are not specified here; they change no
public behavior beyond what the changelog records.

## Synopsis

- `Display` of a wrapping error does not repeat the cause returned by `source()`
  (E1–E3).
- `Error::Io` names its file without debug formatting, and `Error::Parse` becomes
  a struct variant with an optional path, filled in by the file methods (E2, E4).
- `Io` and `Parse` are non-exhaustive variants with public constructors (E5).
- The library enables only the Tokio features `sync`, `rt` and `fs` (T1–T2).
- `f32` fields accept infinity and NaN like `f64` fields (F1–F2).
- `set_string` accepts `impl Into<String>`, the vector setters accept any
  `IntoIterator` (S1–S3).
- `Debug` of `ConfigStore` lists the top-level keys, not the values (D1–D2).
- Unknown keys stay accepted by default; strictness remains opt-in (K1).

## Table of Contents

- [Motivation](#motivation)
- [Goals and non-goals](#goals-and-non-goals)
- [Error messages and sources](#error-messages-and-sources)
- [Tokio features](#tokio-features)
- [Non-finite `f32` values](#non-finite-f32-values)
- [Setter signatures](#setter-signatures)
- [`Debug` of `ConfigStore`](#debug-of-configstore)
- [Unknown keys by default](#unknown-keys-by-default)
- [Compatibility](#compatibility)
- [Alternatives considered](#alternatives-considered)
- [Out of scope](#out-of-scope)
- [Testing](#testing)
- [Documentation deliverables](#documentation-deliverables)
- [Resolved questions](#resolved-questions)
- [Open questions](#open-questions)
- [References](#references)

## Motivation

The 0.1.0 line is meant to be the first API that users can depend on without
following every release. The audit found no defects in the core behavior, but it
found several places where the public surface is inconvenient or surprising:

- error reporters that walk the source chain print the cause twice (A1), and
  `Error::Io` shows `path: Some("config.toml")` (A2);
- an application loading several files cannot tell which one failed to parse (A3);
- every user compiles Tokio's multi-threaded runtime and macros, even when
  the application does not use them (A4);
- `f32` and `f64` fields treat infinity and NaN differently without a reason (A5);
- the setters force callers to create owned values explicitly (A6);
- `{:?}` of a store prints secrets into logs (A7).

All of these change observable behavior and some of them break existing code.
They are cheapest to change now, before the first beta of the 0.1.x line.

## Goals and non-goals

Goals:

- Follow the common Rust conventions for error display and sources, so that
  `anyhow`, `eyre`, `tracing` and similar reporters produce clean messages.
- Let a caller identify the file behind every file-related error.
- Keep the Tokio feature set of the library minimal and documented.
- Make the derive's float handling symmetric.
- Make the setters convenient without changing what they store.
- Avoid leaking configuration values through `Debug`.

Non-goals:

- New configuration features, new field types or new derive attributes.
- A structured operation kind on `Error::Io` (see [Q2](#open-questions)).
- Changing the messages of the key-related errors (`MissingKey`, `TypeMismatch`,
  `ValueParse`, `UnknownKey`); they wrap no source and stay as specified by RFC-0006.

## Error messages and sources

- **E1 — One place for each cause.** An error variant that wraps another error
  returns it from `std::error::Error::source` and does not include its message in
  `Display`. This applies to `Io`, `Parse`, `Serialize` and `SaveFileCleanup`.
  The wrapped errors remain public fields, so callers can still match on them.

- **E2 — Path rendering.** Where a message names a file, the path is rendered with
  `Path::display` and enclosed in double quotes, without escaping. The exact path is
  available in the variant's `path` field; the message is for humans only.

- **E3 — Messages.** `Display` produces exactly these messages (`<p>` is the rendered
  path, `<t>` the rendered temporary path):

  | Variant | Message |
  | --- | --- |
  | `Io { path: Some(_), .. }` | `I/O error on "<p>"` |
  | `Io { path: None, .. }` | `I/O error` |
  | `Parse { path: Some(_), .. }` | `failed to parse TOML file "<p>"` |
  | `Parse { path: None, .. }` | `failed to parse TOML` |
  | `Serialize(_)` | `failed to serialize TOML` |
  | `SaveFileCleanup { .. }` | `failed to save file and to remove its temporary file "<t>" (<cleanup_error>)` |

  `SaveFileCleanup` keeps `save_error` as its source. `cleanup_error` is not part of
  the source chain, so its message stays in `Display`. A report such as
  `anyhow`'s `{:#}` then reads, for example:

  ```text
  failed to save file and to remove its temporary file ".conferencier.1a2b.tmp" (Access is denied. (os error 5)): I/O error on ".conferencier.1a2b.tmp": There is not enough space on the disk. (os error 112)
  ```

- **E4 — Path of parse errors.** `Error::Parse` becomes a struct variant:

  ```rust,ignore
  #[non_exhaustive]
  Parse {
      /// The file that was parsed, if known.
      path: Option<PathBuf>,
      /// The parser's diagnostic, with its location in the document.
      source: toml::de::Error,
  },
  ```

  `ConfigStore::from_file` and `ConfigStore::load_file` set `path` to the path they
  read. `ConfigStore::from_str`, `FromStr` and `From<toml::de::Error>` set it to
  `None`.

- **E5 — Extensible variants and constructors.** Both `Error::Io` and
  `Error::Parse` are individually marked `#[non_exhaustive]`, in addition to the
  existing attribute on `Error` itself. Their `path` and `source` fields remain
  public. Outside the crate, patterns must include `..` and callers cannot
  construct these variants directly. Callers use these public constructors:

  - `Error::io_error(path: Option<PathBuf>, source: std::io::Error) -> Error`;
  - `Error::parse_error(path: Option<PathBuf>, source: toml::de::Error) -> Error`.

  Each constructor stores the supplied path and source in the corresponding
  variant; `None` means that no file path is known. The existing
  `From<std::io::Error>` and `From<toml::de::Error>` conversions remain available
  and set the path to `None`. This rule does not change the other variants.
  Additional fields can later be added without breaking construction or patterns
  if the existing constructors can supply suitable defaults for them.

## Tokio features

- **T1 — Minimal features.** The `conferencier` crate enables only the Tokio
  features `sync`, `rt` and `fs`. `macros` and `rt-multi-thread` move to the
  dev-dependencies, where tests and doctests use them.
- **T2 — Documented promise.** The crate documentation states that the library
  does not start a runtime and does not choose a runtime flavor; the application
  enables the Tokio features it needs, such as `macros` and `rt-multi-thread`. The
  async methods work on any Tokio runtime, including a current-thread runtime.
  Adding a Tokio feature to the library later needs a changelog entry.

## Non-finite `f32` values

- **F1 — Accepted like `f64`.** Loading an `f32` or `Vec<f32>` field accepts the
  TOML values `inf`, `+inf`, `-inf` and `nan` and stores the corresponding `f32`
  value. The `non-finite float` error is removed.
- **F2 — Range unchanged.** A finite value outside the `f32` range remains a
  `ValueParse` error with the message `value out of range for f32`, followed by
  ` (at index N)` for vector elements. Finite values inside the range are rounded
  to the nearest `f32` as before. With F1, every `f32` value saved by
  conferencier loads back to the same value.

## Setter signatures

- **S1 — Strings.** `ConfigStore::set_string` takes `value: impl Into<String>`.
- **S2 — String arrays.** `ConfigStore::set_string_vec` takes
  `values: impl IntoIterator<Item = impl Into<String>>`, so arrays of `&str`,
  vectors of `String` and iterators are accepted.
- **S3 — Other arrays.** `set_integer_vec`, `set_float_vec`, `set_boolean_vec` and
  `set_datetime_vec` take `values: impl IntoIterator<Item = T>` with their current
  element type `T` (`i64`, `f64`, `bool`, `Datetime`). The scalar setters
  `set_integer`, `set_float`, `set_boolean` and `set_datetime` are unchanged; widening
  conversions such as `impl Into<i64>` are not added.

What the setters store, their section validation and their errors are unchanged.

## `Debug` of `ConfigStore`

- **D1 — Keys only.** `ConfigStore` implements `Debug` manually. The output lists
  the top-level keys of the document, in document order, and no values, for example
  `ConfigStore { keys: ["App", "Database"] }`. The alternate form `{:#?}` prints the
  same information, pretty-printed.
- **D2 — Locking.** `Debug` takes the read lock like any other read and follows the
  store's locking rules, including the handling of a poisoned lock. The key names
  are copied under the lock and formatted after it is released, so user code
  behind the formatter never runs while the lock is held.

Applications that need the content use the existing read methods.

## Unknown keys by default

- **K1 — Lenient by default.** RFC-0006 Q6 left "strict by default" as a decision
  before 0.1.0. This RFC keeps the current default: derived types accept unknown
  keys unless they opt in with `deny_unknown_keys`. No `allow_unknown_keys`
  attribute is added. Reasons: a configuration file shared by several scopes or
  written by a newer application version must keep loading; strictness is a
  per-type choice that the derive already supports; and making strictness the
  default after 0.1.0 would be a breaking change, so the decision has to be made
  now rather than deferred again.

## Compatibility

This RFC changes behavior that users can observe. It is released in
0.1.0-beta.1, the first release of the 0.1.x line.

- **Breaking:** `Display` of `Io`, `Parse`, `Serialize` and `SaveFileCleanup` no
  longer contains the cause (E1, E3). Code that prints an error with `{}` and relies
  on the cause must print the source chain, for example with `anyhow`'s `{:#}` or by
  walking `source()`. Code that compares message strings must be updated.
- **Breaking:** `Error::Io` is non-exhaustive (E5). Replace direct construction
  with `Error::io_error(path, source)` and add `..` to patterns such as
  `Error::Io { path, source, .. }`.
- **Breaking:** `Error::Parse` is a non-exhaustive struct variant (E4–E5).
  Patterns `Error::Parse(_)` become `Error::Parse { .. }` or
  `Error::Parse { source, .. }`. Replace direct construction with
  `Error::parse_error(None, source)` or the existing `Error::from(source)`.
- **Breaking:** the library no longer enables Tokio's `macros` and `rt-multi-thread`
  features (T1). An application that used `#[tokio::main]`, `#[tokio::test]` or the
  multi-threaded runtime without enabling these features itself fails to compile
  until it adds them to its own `tokio` dependency.
- **Breaking:** calls such as `set_string("App", "name", "demo".into())` or
  `set_string_vec("App", "langs", vec!["en".into()])` no longer infer the target
  type (S1, S2). Pass the value directly: `set_string("App", "name", "demo")`.
  Calls that pass a `String` or `Vec<String>` keep compiling.
- **Breaking:** the `Debug` output of `ConfigStore` no longer contains values (D1).
- `f32` fields that previously failed with `non-finite float` now load (F1). An
  application that relied on this rejection must check the value itself.
- K1 changes nothing.

`Error` is already `#[non_exhaustive]`; no variant is added or removed.

## Alternatives considered

1. **Keep the cause in `Display` and return `None` from `source()`** (A1). Printing
   with `{}` would stay informative, which helps applications that print errors
   without a reporter, such as the example applications. It hides the cause from
   reporters, `tracing` and `anyhow::Error::chain`, and it is the less common
   convention for wrapping errors. Rejected in favor of E1, but see
   [Q1](#open-questions).
2. **An operation field on `Error::Io`** (read, write, replace, …) (A2). It would
   allow messages such as `failed to read "config.toml"`, but the save path alone
   has several distinct operations (metadata, temporary file creation, writing,
   permissions, rename), and the variant would gain a public enum that has to stay
   stable. The path and the `io::ErrorKind` identify the problem well enough.
   Deferred, see [Q2](#open-questions).
3. **A new variant `ParseFile { path, source }`** (A3). It would keep
   `Parse(toml::de::Error)` source-compatible, but matching every parse error would
   need two patterns, and `Io` already uses an optional path. Rejected.
4. **Wrapping a file's parse error in `Io` with `ErrorKind::InvalidData`.** It would
   mix syntax errors with I/O failures and lose the `toml::de::Error` type. Rejected.
5. **Documenting the `f32` asymmetry instead of removing it** (A5). The asymmetry
   has no technical reason, and it breaks the round trip of saved values (RFC-0007
   S2). Rejected.
6. **`impl Into<…>` element types for every vector setter** (S3). Integer and float
   conversions would allow `[1u8, 2]`, but they are inconsistent with the unchanged
   scalar setters and make some literal arrays ambiguous. Rejected.
7. **Full document with the alternate flag `{:#?}`** (D1). `{:#?}` is commonly used
   for pretty logs, so it would leak secrets just as easily. Rejected.
8. **Removing `Debug` from `ConfigStore`.** It would break `#[derive(Debug)]` on
   application types that contain a store. Rejected.
9. **Strict by default with an `allow_unknown_keys` opt-out** (K1). Catches typos
   earlier, but breaks shared and forward-compatible files by default. Rejected.
10. **Leave `Io` and `Parse` directly constructible.** This avoids the migration
    in E5, but adding a field would break construction and patterns that list all
    fields. Rejected in favor of non-exhaustive variants and constructors.

## Out of scope

- The internal findings A8 and B1–B5 of the audit; they follow the plan in the
  [backlog](../BACKLOG.md#planned-for-010-beta1).
- A redaction mechanism for individual values or fields.
- A blocking variant of `from_file`.
- Changes to the key-related error messages.

## Testing

- Unit tests in `error.rs` assert every message of E3, with and without a path,
  and assert that `source()` returns the wrapped error for each wrapping variant.
- A test asserts that no wrapping variant's `Display` contains the message of its
  source.
- Tests of `from_file` and `load_file` with invalid TOML assert
  `Error::Parse { path: Some(p), .. }` with the read path; a `from_str` test asserts
  `path: None`.
- Tests outside the library crate construct `Io` and `Parse` with the public
  constructors, with and without paths, and inspect their fields using patterns
  with `..`. Compile-fail tests outside the crate verify that direct construction
  and patterns without `..` are rejected for both variants.
- `cargo test -p conferencier --no-default-features --locked` and the workspace tests
  pass with the reduced Tokio features; a test or doctest runs an async method on
  a current-thread runtime.
- Derive tests load `inf`, `-inf` and `nan` into `f32` and `Vec<f32>` fields and
  keep the out-of-range tests, including the vector index in the message. A round
  trip of a non-finite `f32` default through save and load succeeds.
- Setter tests pass `&str`, `String`, arrays of `&str` and iterators.
- A test asserts that the `Debug` output contains the top-level keys and no value
  of the document.

## Documentation deliverables

- Rustdoc of `Error` (variants, constructors and the I/O example), `ConfigStore`
  (`Debug`, setters, `from_file`, `load_file`) and the crate-level note about
  Tokio features (T2).
- Guide: printing errors with their source chain in
  [troubleshooting](../../conferencier/guide/troubleshooting.md); the I/O error
  construction example in
  [concepts](../../conferencier/guide/concepts.md#reading-a-file-before-the-runtime-starts);
  the `f32` rules in [derive](../../conferencier/guide/derive.md#value-conversion) and
  the round-trip note in [files](../../conferencier/guide/files.md).
- The example applications print errors with their source chain.
- [Migration guide](../../conferencier/guide/migration.md): a 0.0.8 → 0.1.0-beta.1
  section covering every **Breaking** item of [Compatibility](#compatibility).
- [Changelog](../../CHANGELOG.md): the **Breaking** entries and the `f32` change
  under [Unreleased].
- `Amended by: RFC-0011` in the header of RFC-0001, once this RFC is approved.

## Resolved questions

- **Q5 — Extensibility of `Io` and `Parse`.** Review noted that the
  `#[non_exhaustive]` attribute on `Error` protects against new variants, not new
  fields in existing variants. E5 makes these two variants non-exhaustive and
  provides constructors, allowing fields to be added when suitable defaults
  exist. `SaveFileCleanup` is considered separately with save-file diagnostics.

## Open questions

- **Q1 — Cause in `Display` or in `source()`?** E1 follows the convention of the
  Rust error-handling project group and works best with reporters, but an application printing
  `{}` sees only `I/O error on "config.toml"` without the reason. Alternative 1 is
  the opposite trade-off. Which side should conferencier take?
- **Q2 — Operation in `Error::Io`?** Is the path enough, or should the message say
  what failed (read, write, replace)?
- **Q3 — Debug content.** Are top-level keys enough, or should `Debug` also list
  nested table names, or print only the number of sections?
- **Q4 — Strict by default.** Is the decision of K1 (keep lenient) acceptable for
  0.1.0, or should it be reconsidered before the stable release?

## References

- [AUDIT-001: Quality audit of 0.0.8](../devel-0.0.x/AUDIT-001_quality_0.0.8.md), findings A1–A7.
- [RFC-0001: Baseline architecture](../devel-0.0.x/RFC-0001_baseline_architecture.md): the original `Error`, Tokio features and `f32` rule.
- [RFC-0006: Strict unknown keys](../devel-0.0.x/RFC-0006_strict_unknown_keys.md), Q6.
- [RFC-0007: Default expressions](../devel-0.0.x/RFC-0007_default_expressions.md), S2.
- Rust error-handling project group, [guidelines for `source` and `Display`](https://blog.rust-lang.org/inside-rust/2021/07/01/What-the-error-handling-project-group-is-working-towards/#guidelines-for-implementing-display-fmt-and-error-source).
