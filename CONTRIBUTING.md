# Contributing

Thanks for your interest in improving **conferencier**! This quick guide explains how to get set up and submit changes.

## Development workflow

1. **Clone** the repository and install the latest stable Rust toolchain (the project targets the 2024 edition; the minimum supported Rust version is 1.85, so language features and standard library APIs stabilized later, such as let chains, cannot be used).
2. **Run tests** locally before opening a pull request:

   ```powershell
   cargo test --workspace --all-features
   cargo test -p conferencier --no-default-features
   cargo test -p conferencier --features toml/preserve_order --test scope_integration strict_scope_reports_smallest_unknown_key_with_preserved_order --locked -- --ignored
   ```

   The first command covers the whole workspace, including the compile-fail tests of the derive macro and the README examples (see [Documentation](#documentation)). The trybuild snapshots (`tests/derive/tests/trybuild/*.stderr`) follow the latest stable Rust. If you change a diagnostic on purpose, rerun the tests with the `TRYBUILD=overwrite` environment variable set and review the updated files. The same cases with Rust 1.85 (MSRV) snapshots live in `tests/derive-msrv/`; this suite is ignored by default, so update it as well with `cargo +1.85 test -p derive-msrv-tests --locked -- --ignored`. Changes to a test case's `.rs` file must be copied to both suites.

   The last command runs the otherwise ignored strict-ordering test with `toml/preserve_order`; it checks that the feature actually preserves insertion order before testing the reported key.

3. **Check formatting, lints and docs**:

   ```powershell
   cargo fmt --all
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo doc --workspace --no-deps
   ```

   CI runs tests on Linux, Windows and macOS, and runs formatting, lint and documentation checks on Linux. It also checks the minimum supported Rust version and treats rustdoc warnings as errors (set `RUSTDOCFLAGS=-D warnings` to do the same locally). CI runs the doctests of the packaged `conferencier` crate outside the workspace, because `cargo publish --dry-run` does not run them.

4. **Document your changes** by updating `CHANGELOG.md`, the [guide](#documentation) and rustdoc comments or examples when appropriate.
5. **Open a pull request** against the `develop` branch with a clear description of the change, including any follow-up work or known limitations.

## 0.1.x compatibility and release checks

The 0.1.x compatibility boundary is the minor version: every pre-release and
release must remain backward compatible with the immediately preceding
published 0.1.x version. Breaking changes, including major-version upgrades
of public `toml` or `tokio` dependencies, are deferred to 0.2.0. Rust 1.85 is
supported throughout 0.1.x.

The MSRV checks validate both the locked dependency graph and a fresh,
MSRV-aware dependency resolution in fallback mode on Rust 1.85. The fresh
resolution uses an isolated lockfile and must not change the workspace
`Cargo.lock`.

Before publishing `0.1.0-beta.1`, audit the intentional transition from 0.0.8
with `cargo-semver-checks`, using `--release-type minor` explicitly:

```powershell
cargo semver-checks --package conferencier --baseline-version 0.0.8 --release-type minor --default-features
cargo semver-checks --package conferencier --baseline-version 0.0.8 --release-type minor --only-explicit-features
```

This is a reviewed audit, not a requirement that the transition have no
reported breaks. Reconcile every break with the compatibility and migration
documentation. After the beta.1 tag exists, compare each candidate with the
immediately preceding 0.1.x tag using `--baseline-rev <tag>` and
`--release-type minor`, using the same two feature configurations shown above.
The baseline includes pre-releases and advances after each publication.

`cargo-semver-checks` cannot check `conferencier-derive`: a proc-macro crate
has no library API for it to compare. The derive's compatibility is covered by
the trybuild pass and fail tests, the packaged-consumer builds below, the exact
version requirement in `conferencier`, and a manual review of accepted derive
inputs, generated code and the matching `__private` support.

Before every 0.1.x pre-release or release, also build three downstream
consumers on Rust 1.85 against the actual packaged `.crate` archives for both
crates, outside the workspace. A temporary local registry or `[patch]` entries
to extracted package directories may resolve the archives; do not use workspace
package paths:

1. Use the `conferencier` default features and derive a scope through the
   crate-root macro re-export.
2. Disable `conferencier`'s default features and compile manual `ConfigScope`
   and `ConfigTable` implementations.
3. Disable the core default features, pin both crates to the same exact
   candidate version, import both macros from `conferencier-derive`, and derive
   both traits.

All three builds must succeed. This packaged-consumer check is a release gate,
not a per-pull-request CI job.

Use absolute docs.rs links to versioned documentation, not `/latest/`. During
the 0.1.x series use the range URL beginning
`https://docs.rs/conferencier/%5E0.1.0/`; update it when the next
development series begins. Before publication, inspect both packaged archives
and confirm they contain no `docs.rs/conferencier/latest` links.

## Documentation

- **`README.md`** is a short showcase shared by GitHub and crates.io. Cargo copies it into the root of the `conferencier` package, so its links must be absolute URLs. Its Rust code blocks run as doctests of the unpublished `docs-tests` workspace member in `tests/docs/`.
- **`tests/`** holds unpublished workspace members for test suites that need to test across crates or use repository files. `derive-tests` runs the derive macro's trybuild suite against both library crates and checks with clippy that generated code triggers no pedantic lints in user crates, `derive-msrv-tests` runs the same suite with Rust 1.85 diagnostics; `docs-tests` runs the README code blocks and the examples of `conferencier-derive`. Those examples live in `conferencier-derive/doc/` and are included into the macro documentation, because the derive package cannot depend on the core crate for its own doctests.
- **Documentation examples must compile without warnings.** `conferencier` and `docs-tests` run their doctests with `deny(warnings)`; use hidden `# ` lines for setup and to use items the example only defines. Mark a block `ignore` only for code of an old API version in the migration guide.
- **`conferencier/guide/`** holds the user guide in Markdown. `overview.md` is the crate-level documentation; the other pages become modules of `conferencier::guide` (see `conferencier/src/guide.rs`) and are listed in `index.md`. A new page needs both. The guide is part of the package, so its Rust code blocks run as doctests and its intra-doc links are checked by `cargo doc`; link between pages and to API items with intra-doc links, not with file paths.
- **Migration steps** for every release belong to `conferencier/guide/migration.md`; `CHANGELOG.md` only links to them.
- **`docs/`** contains the [backlog](docs/BACKLOG.md) and the design records (RFCs, implementation reports and audits) grouped by version series, such as `docs/devel-0.0.x/`.
- **`examples/`** holds runnable example applications, such as [`uptime-monitor`](examples/uptime-monitor/README.md), [`folder-indexer`](examples/folder-indexer/README.md) and [`modular-agent`](examples/modular-agent/README.md). Each is an unpublished workspace member (`publish = false`) documented in its own `README.md`, so its dependencies stay out of the library crates. The workspace commands above build, lint and test it; run it with `cargo run -p <name>`. Short examples of single features belong to `conferencier/examples/` instead.

## Reporting issues

- Use the issue tracker to describe bugs, desired features, or documentation gaps.
- Include reproduction steps, expected vs. actual behavior, and environment details when possible.
- Do not report security vulnerabilities in public issues; follow the [security policy](SECURITY.md) instead.

## Code of conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). Be respectful and collaborative. Assume good intent, provide constructive feedback, and help keep the project welcoming for everyone.

Happy hacking!
