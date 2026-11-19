# AUDIT-001: Quality audit of 0.0.8

Date: 2026-11-17
Status: Closed
Author: Komix42 <komix[at]ignore.cz>
Audited release: 0.0.8 (commit a363dc7)
Target release: 0.1.0-beta.1

## Document Scope

This report records the findings of a quality audit of the `conferencier` and
`conferencier-derive` crates at release 0.0.8, made before the planned move to
0.1.0. It records the state found and the decision for each finding; it does not
specify the fixes. Changes of public behavior are specified by a separate RFC
(see [Follow-up](#follow-up)). Like the RFCs, this report is a historical record:
once closed, only its Status line and cross-references change.

Topics already tracked in the [backlog](../BACKLOG.md), such as file
synchronization in `save_file` or Windows access control, are not repeated here.

## Method

- Review of the library sources, the derive macro, the manifests, CI and the user documentation.
- All CI gates were run locally on Windows with stable Rust 1.99 and passed:
  formatting, Clippy with `-D warnings`, rustdoc with `-D warnings`, workspace
  tests with all features, core tests without default features, the MSRV
  check with Rust 1.85 and `cargo audit` (no advisories).
- Suspected defects were verified with a scratch crate outside the repository
  that depended on the workspace by path. Findings marked *verified* were
  reproduced this way; the others follow from reading the code.

## Summary

The project is in good shape: the gates pass, the file replacement in
`save_file` is carefully hardened, scope loading is atomic, diagnostic paths are
unambiguous, the derive has extensive compile-fail tests and the documentation is
thorough. The findings below are mostly small, but several of them change public
behavior, so they should be resolved before the API is stabilized in 0.1.0.

Decisions:

- **RFC** — changes public behavior or API; specified by the follow-up RFC.
- **Fix** — internal change without an RFC; recorded in the changelog if users notice it.
- **Backlog** — deferred; moved to the backlog with a link to this report.

## Findings: public behavior and API

### A1. Error sources are reported twice

`Display` for `Error::Io`, `Parse`, `Serialize` and `SaveFileCleanup` includes the
message of the underlying error, which `source()` also returns
([error.rs](../../conferencier/src/error.rs), `impl fmt::Display for Error`).
Error reporters that walk the source chain, such as `anyhow` with `{:#}`, print
the same message twice. *Verified* (the localized OS message is translated):

```text
I/O error (path: Some("does-not-exist.toml")): The system cannot find the file specified. (os error 2): The system cannot find the file specified. (os error 2)
```

The usual convention (Rust API guidelines, error-handling working group) is to
expose the cause either in `Display` or through `source()`, not both.

Decision: **RFC**. Messages are part of observable behavior and tests compare them.

Resolution: [RFC-0011](../devel-0.1.x/RFC-0011_api_polish_before_0.1.md#error-messages-and-sources),
released in 0.1.0-beta.1.

### A2. `Error::Io` displays a debug-formatted `Option`

The message is built as `I/O error (path: {path:?})`, so users see
`path: Some("config.toml")` or `path: None`. A message such as
`failed to read config.toml` is clearer, and the operation (read, write,
replace) would help as well.

Decision: **RFC**, together with A1.

Resolution: [RFC-0011](../devel-0.1.x/RFC-0011_api_polish_before_0.1.md#error-messages-and-sources),
released in 0.1.0-beta.1.

### A3. Parse errors of files do not name the file

`ConfigStore::from_file` and `load_file` return `Error::Parse` without the
path. This is documented, but an application with several configuration files
cannot tell which one is invalid without wrapping every call.

Decision: **RFC**. Attaching the path needs a field or variant change in the
non-exhaustive `Error` enum.

Resolution: [RFC-0011](../devel-0.1.x/RFC-0011_api_polish_before_0.1.md#error-messages-and-sources),
released in 0.1.0-beta.1.

### A4. The library enables unneeded Tokio features

[conferencier/Cargo.toml](../../conferencier/Cargo.toml) enables `sync`, `rt`,
`rt-multi-thread`, `macros` and `fs` on Tokio. The library itself needs only
`sync`, `fs` and `rt` (for `spawn_blocking`); `macros` and `rt-multi-thread` are
used only by tests and doctests and belong in the dev-dependencies. Because
Cargo features are additive, every user compiles them.

Decision: **RFC**. Removing features can break users who rely on them
transitively; the RFC also states the minimal feature set as a documented promise.

Resolution: [RFC-0011](../devel-0.1.x/RFC-0011_api_polish_before_0.1.md#tokio-features),
released in 0.1.0-beta.1.

### A5. `f32` fields reject infinity and NaN, `f64` fields accept them

The generated conversion of `f32` and `Vec<f32>` returns
`invalid value at A.x: non-finite float` for `inf`, `-inf` and `nan`
([codegen.rs](../../conferencier-derive/src/codegen.rs), `float_from_store` and
`float_vec_from_store`), while `f64` accepts them. *Verified.* `f32` can represent
these values, so the asymmetry is not needed for correctness.

Decision: **RFC**. Either accept non-finite values for `f32` (only finite values
outside the `f32` range are errors) or document the rule for both types.

Resolution: [RFC-0011](../devel-0.1.x/RFC-0011_api_polish_before_0.1.md#non-finite-f32-values)
accepts non-finite `f32` values, released in 0.1.0-beta.1.

### A6. Typed setters take owned `String` and `Vec`

`ConfigStore::set_string(&self, section, key, value: String)` and the vector
setters require owned values, so callers write `"x".to_owned()` or
`String::from("x")`. `impl Into<String>` (and similar for the vectors) is more
convenient.

Decision: **RFC**. A signature change; generic parameters can affect type
inference in existing calls.

Resolution: [RFC-0011](../devel-0.1.x/RFC-0011_api_polish_before_0.1.md#setter-signatures),
released in 0.1.0-beta.1.

### A7. `Debug` of `ConfigStore` prints the whole document

`ConfigStore` derives `Debug` ([store.rs](../../conferencier/src/store.rs)), so
`{:?}` prints every value, including secrets such as passwords or tokens, into
logs. A manual implementation could print only the section names, or the
document only with the alternate flag.

Decision: **RFC**.

Resolution: [RFC-0011](../devel-0.1.x/RFC-0011_api_polish_before_0.1.md#debug-of-configstore),
released in 0.1.0-beta.1.

### A8. The derive dependency is not pinned to an exact version

The workspace declares `conferencier-derive = { version = "0.0.8", ... }`, a caret
requirement. The generated code calls `conferencier::__private`, which has no
stability promise, so `conferencier` and `conferencier-derive` must always come
from the same release. With pre-releases, a caret requirement can pair different
betas. Crates with the same structure, such as `serde`, use `=` requirements.

Decision: **Fix**, before 0.1.0-beta.1: use `=` in the workspace dependency.

Resolution: fixed in 0.1.0-beta.1 (`=0.1.0-beta.1`); the policy is specified in
[RFC-0011](../devel-0.1.x/RFC-0011_api_polish_before_0.1.md#versioning-and-public-dependencies).

## Findings: internal quality

### B1. Generated code is not fully hygienic

The derive mostly uses absolute paths (`::core::option::Option`, `::std::vec::Vec`),
but some generated code uses unqualified names: `format!`, `Vec::with_capacity`
(`integer_vec_from_store`, `float_vec_from_store`), `String::from`, `Ok`, `Err`
and `f32::MIN`/`f32::MAX` ([codegen.rs](../../conferencier-derive/src/codegen.rs)).
A user item with the same name in the deriving module breaks the derive.
*Verified* with a local `macro_rules! format` and a local `struct Vec`; both
fail to compile.

Decision: **Fix**. Use absolute paths everywhere and add a trybuild pass test
that shadows these names.

Resolution: fixed in 0.1.0-beta.1 for the `core` and `std` paths, with the
trybuild pass test `pass_generated_hygiene`. The path to the `conferencier`
crate itself remained relative; see
[AUDIT-002](../devel-0.1.x/AUDIT-002_quality_0.1.0-beta.1.md) A1.

### B2. Unused dev-dependency `once_cell`

`once_cell` is listed in the dev-dependencies of `conferencier` but not used.
`std::sync::LazyLock` (Rust 1.80) covers its use case within the MSRV.

Decision: **Fix**.

Resolution: removed in 0.1.0-beta.1.

### B3. Dead branch in `annotate_with_index`

In [value_conversion.rs](../../conferencier/src/value_conversion.rs),
`annotate_with_index` rebuilds `Error::TypeMismatch` unchanged, so it never adds
the index its name promises. Element mismatches are reported as `ValueParse`,
so the branch is effectively unreachable.

Decision: **Fix**. Remove the branch or make it annotate consistently.

Resolution: the branch was removed in 0.1.0-beta.1.

### B4. Unused `doc_cfg` attribute

`#![cfg_attr(docsrs, feature(doc_cfg))]` is enabled in
[lib.rs](../../conferencier/src/lib.rs), but no item uses `doc(cfg(...))`, and
the manifest has no `[package.metadata.docs.rs]` that sets `--cfg docsrs`.
Items behind the `derive` feature are therefore not marked as such on docs.rs.

Decision: **Fix**. Either configure docs.rs and mark the `derive` items, or remove the attribute.

Resolution: the attribute was removed in 0.1.0-beta.1.

### B5. Field types are classified by name only

The derive recognizes field types by the last path segment
([parser.rs](../../conferencier-derive/src/parser.rs), `classify_scalar`,
`type_ident`):

- type aliases (`type Port = u16;`) are rejected as unsupported types;
- `Vec<Vec<T>>` is reported as an unsupported type with a hint to use
  `#[confer(table)]`, which does not apply;
- references such as `&'static str` pass the parser and fail later in the
  generated code with a less helpful error;
- conversely, a user type named, for example, `Datetime` or `String` is treated as the built-in type.

A procedural macro cannot resolve types, so the rule cannot be removed, but it
should be documented and the diagnostics improved.

Decision: **Fix** for the diagnostics and the documentation; supporting more
types belongs to the backlog entry *Conversion hooks*.

Resolution: fixed in 0.1.0-beta.1; the derive reports type aliases, references
and nested vectors with specific diagnostics, and the
[derive guide](../../conferencier/guide/derive.md) documents the naming rule.

## Not taken up

- **Size of the design records.** The RFCs are larger than the code. This is
  expected in 0.0.x, which uses the absence of backward compatibility for fast
  design iteration; the process for 0.1.x can be reconsidered separately.

## Follow-up

1. An RFC for findings A1–A7, targeting 0.1.0-beta.1.
2. Fixes A8 and B1–B5, recorded in the changelog where users notice them.
3. Release 0.1.0-beta.1 as a candidate for the stable API:
   - A pre-release is not selected by `cargo add` or by a requirement such as
     `"0.0.8"`, so only users who request it get it. A requirement such as
     `"0.1.0-beta.1"` accepts later betas and the final 0.1.0, so breaking
     changes between betas should still be avoided where possible.
   - 0.1.0 follows once the beta has been used in the example applications and
     no further breaking changes are pending.
4. Close this report: set its Status to Closed and link the RFC and the release
   that resolved each finding.
