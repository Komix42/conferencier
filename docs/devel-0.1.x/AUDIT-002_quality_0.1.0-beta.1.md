# AUDIT-002: Quality audit before 0.1.0-beta.1

Date: 2026-11-19
Status: Open
Author: Komix42 <komix[at]ignore.cz>
Audited release: 0.1.0-beta.1 candidate (commit 0ee5d7f)
Target release: 0.1.0-beta.1

## Document Scope

This report records an independent quality audit of the `conferencier` and
`conferencier-derive` crates at the 0.1.0-beta.1 candidate, after the
implementation of [RFC-0011](./RFC-0011_api_polish_before_0.1.md). Its focus is
the API freeze: `0.1.0-beta.1` is the first release covered by the 0.1.x
compatibility commitment, so every later 0.1.x release must stay backward
compatible with it and any breaking change is deferred to 0.2.0. A decision
that is cheap today becomes a 0.2.0 item after publication.

The report records the state found and the decision for each finding; it does
not specify the fixes. Like the RFCs, it is a historical record: once closed,
only its Status line and cross-references change.

Findings of [AUDIT-001](../devel-0.0.x/AUDIT-001_quality_0.0.8.md) resolved by
RFC-0011, and topics already tracked in the [backlog](../BACKLOG.md), are not
repeated here unless the freeze changes their weight.

## Method

- Review of the public API of both crates (`lib.rs`, `error.rs`, `store.rs`,
  `scope.rs`, `table.rs`, `table_path.rs`, `value_conversion.rs`), the derive
  macro (`parser.rs`, `codegen.rs`, `crate_path.rs`), the manifests, CI, the
  README, the rustdoc guide, the changelog, CONTRIBUTING and RFC-0011's
  compatibility policy. Each public item was checked against the question
  "what can still change compatibly in 0.1.x?".
- All CI gates were run locally on Windows and passed: formatting, Clippy with
  `-D warnings`, rustdoc with `-D warnings`, workspace tests with all features
  (including trybuild and doctests), core tests without default features,
  the Rust 1.85 checks and core tests, and `cargo audit` (no advisories), with
  stable Rust 1.99.1.
- Not run: `cargo-semver-checks` (not installed locally; it is a separate beta
  release step, see [the backlog](../BACKLOG.md#beta1-release-steps)), the
  fresh MSRV dependency resolution, the packaged-consumer checks and the
  publish dry-runs. Only Windows was tested.
- Suspected defects were checked with a scratch crate outside the repository
  that depended on the workspace by path. Findings marked *verified* were
  reproduced this way; the others follow from reading the code.

## Summary

The candidate is in very good shape. All gates pass, the error type is
structured for evolution (`#[non_exhaustive]` enum and variants, constructors,
compile-fail doctests that guard extensibility), the compatibility policy is
explicit about traits, derive output, `__private`, `Display` text and the
public `toml` and Tokio dependencies, and the documentation is thorough and
consistent with the code.

One finding needs a decision before beta.1 because either outcome is hard to
change afterwards: how the generated code names the `conferencier` crate (A1).
A few further items are design choices that the freeze makes permanent for
0.1.x; they need no code change, only an explicit confirmation (A2–A5). The
remaining findings are small documentation and test fixes.

Decisions:

- **Fix** — change before beta.1 without an RFC; recorded in the changelog if users notice it.
- **Confirm** — keep the current design; the freeze makes it permanent for 0.1.x. Documented where useful.
- **Backlog** — deferred; moved to the backlog with a link to this report.
- **Not taken up** — no action.

## Findings: decide before the API freeze

### A1. Generated code names the core crate with a relative path

[crate_path.rs](../../conferencier-derive/src/crate_path.rs) resolves the core
crate with `proc-macro-crate` and then explicitly clears the leading `::`, so
the derives emit paths such as `conferencier::__private::...` and
`conferencier::Error`. A relative path is resolved in the user's module, so any
item named `conferencier` there shadows the crate. *Verified*: a private
`mod conferencier {}` next to `#[derive(ConfigTable)]` fails with five errors
such as ``cannot find `Error` in `conferencier` ``. A `use other as
conferencier;` would silently redirect the generated code to another crate. The
hygiene tests in
[scope_integration.rs](../../conferencier/tests/scope_integration.rs)
(`default_expression_hygiene`) do not cover this.

The relative path also has an effect that users may come to rely on: when the
dependency is not found (`Err` branch, for example a facade crate that
re-exports `conferencier`), the generated code compiles if the user brings a
`conferencier` name into scope. Once beta.1 is out, both directions are
breaking under RFC-0011's derive policy: switching to `::conferencier` breaks
such facade users, and keeping the relative path freezes the shadowing hazard.

Decision: **Fix** before beta.1. Emit an absolute `::name` when Cargo resolves
the dependency (including a renamed one), as AUDIT-001 B1 already did for
`core` and `std` paths; keep the relative fallback only when the dependency is
not found, and document it. Add a hygiene test with a local
`mod conferencier`. An explicit `#[confer(crate = "...")]` attribute for facade
crates is additive and goes to the **Backlog**.

### A2. Scopes can only be constructed as shared values

`ConfigScope::from_store` returns `SharedScope<Self>`
([scope.rs](../../conferencier/src/scope.rs)), and the trait cannot gain methods
in 0.1.x. Code that wants a plain value, such as a test, a validation step or a
one-shot command-line tool, has no scope-level way to get one. *Verified*
workaround: derive both `ConfigScope` and `ConfigTable` on the struct and call
`App::from_table("App", store.get_section("App").unwrap_or_default())`, which
duplicates the section name.

A future derive-generated inherent method would be breaking (it can collide with
user methods), so any later addition must be a free function or a separate
extension trait.

Decision: **Confirm**. Document the dual-derive workaround in the guide;
a free-standing helper can be added in 0.1.x if a need appears (**Backlog**).

### A3. `load` and `save` are associated functions over `SharedScope`

`AppConfig::load(&app, &store)` cannot be written as `app.load(&store)`, because
`SharedScope<T>` is a type alias for `Arc<tokio::sync::RwLock<T>>`. The alias, the
`Send + Sync + Sized + 'static` supertraits and the `impl Future + Send` return
types are all frozen by the trait-evolution rule. The choice is consistent and
documented; it is recorded here because a newtype around the lock, which would
allow method syntax and hide Tokio from the signature, is no longer possible in
0.1.x.

Decision: **Confirm**.

### A4. Blanket implementations of `ConfigTable` would be breaking

The backlog entry "More table shapes" lists `Box<T>` and `Option<Vec<T>>` table
fields. Implementing them as `impl<T: ConfigTable> ConfigTable for Box<T>` would
be a breaking change, because `Box` is a fundamental type and downstream crates
may already implement `ConfigTable` for `Box<TheirType>`. The same applies to
other blanket implementations of `ConfigScope` or `ConfigTable`.

Decision: **Backlog**. Extend the entry with the constraint that new table shapes
are recognized by the derive, not added through blanket trait implementations.

### A5. Signatures and lenient conversions that become permanent

The following details are reasonable and documented, but cannot change in
0.1.x; they are listed so that the freeze is a conscious choice:

- `remove_section` returns `()` and `remove_key` returns `Result<()>`; neither
  reports whether something was removed or returns the removed value.
- `get_datetime` and `get_datetime_vec` parse strings as datetimes, and the
  float getters accept integers ([value_conversion.rs](../../conferencier/src/value_conversion.rs)).
  The derive uses the same rules ([derive guide](../../conferencier/guide/derive.md)).
- The key-error fields `expected` and `found` of `TypeMismatch` are
  `&'static str`, and `ValueParse.message` is an unstructured `String`. New
  fields can still be added, because the variants are non-exhaustive.
- The file methods take `impl AsRef<Path> + Send + Sync`; relaxing the bounds
  later is compatible, tightening them is not.

Decision: **Confirm**.

## Findings: documentation and tests

### B1. The changelog links to `/latest/` docs

The [changelog](../../CHANGELOG.md) entry of 0.0.7 for the rustdoc guide links to
`https://docs.rs/conferencier/latest/conferencier/guide/`. docs.rs serves the
newest stable version at `/latest/`, so the link of a historical entry moves to
later releases as they are published. CONTRIBUTING requires versioned links; the
release step only inspects the packaged archives, so the root changelog is not
caught.

Decision: **Fix**. Link the guide of the release that the entry describes, 0.0.7.

### B2. Error-message list in the troubleshooting guide has no heading

[troubleshooting.md](../../conferencier/guide/troubleshooting.md) opens with
"Common compile-time and load-time error messages", but the list of messages
follows the error-chain helper under the heading "Showing error causes", so it
looks like part of that section and has no anchor of its own.

Decision: **Fix**. Move the list under its own heading, such as
"Common error messages".

### B3. `Send` futures of the file methods are not tested

`ConfigScope::load` and `save` declare `+ Send` in their signatures, but the
futures of `ConfigStore::from_file`, `load_file` and `save_file` are `Send` only
through auto-trait leakage of `async fn`. *Verified* that they are `Send` today,
but no test asserts it, so an internal change (for example holding a lock guard
across an `.await`) could break users who spawn them on Tokio tasks without any
gate failing. The same holds for `Send + Sync` of `ConfigStore`; only `Error`
has such a test.

Decision: **Fix**. Add a compile-time assertion for these futures and for
`ConfigStore`.

## Not taken up

- **Inherent `ConfigStore::from_str` next to `FromStr`.** Intentional for
  discoverability; both are documented.
- **Path escaping in `Display`.** File paths use Rust escapes such as `\u{1b}`,
  table paths TOML escapes such as `\u001B`. `Display` text is not stable, so it
  can be unified later.
- **Type classification by name in the derive.** Documented; a user type named
  `String` or `Datetime` produces a compile error, not a silent misbehavior. A
  trait-based conversion would be additive.
- **`toml` and Tokio in the public API.** Documented in the compatibility policy;
  a major update of either is a 0.2.0 change.
- **Ordering of `list_sections` and `list_keys`.** Documented as unspecified.

## Follow-up

- A1, B1, B2 and B3 are fixed, and the A2 workaround is documented, before
  beta.1, as part of the review step in the
  [beta.1 release steps](../BACKLOG.md#beta1-release-steps).
- A2 and A4, and the `crate` attribute from A1, are recorded in the
  [backlog](../BACKLOG.md) with a link to this report.
- The report is closed when each finding links to the change or backlog entry
  that resolved it.
