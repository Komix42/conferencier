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

   The first command covers the whole workspace, including the compile-fail tests of the derive macro and the README examples (see [Documentation](#documentation)). Their expected compiler output (`conferencier-derive/tests/trybuild/*.stderr`) follows the latest stable Rust. If you change a diagnostic on purpose, rerun the tests with the `TRYBUILD=overwrite` environment variable set and review the updated files.

   The last command runs the otherwise ignored strict-ordering test with `toml/preserve_order`; it checks that the feature actually preserves insertion order before testing the reported key.

3. **Check formatting, lints and docs**:

   ```powershell
   cargo fmt --all
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo doc --workspace --no-deps
   ```

   CI runs the same checks on Linux, Windows and macOS, checks the minimum supported Rust version and treats rustdoc warnings as errors (set `RUSTDOCFLAGS=-D warnings` to do the same locally). It also runs the doctests of the packaged `conferencier` crate, outside the workspace, because `cargo publish --dry-run` does not run them.

4. **Document your changes** by updating `CHANGELOG.md`, the [guide](#documentation) and rustdoc comments or examples when appropriate.
5. **Open a pull request** against the `develop` branch with a clear description of the change, including any follow-up work or known limitations.

## Documentation

- **`README.md`** is a short showcase shared by GitHub and crates.io. Cargo copies it into the root of the `conferencier` package, so its links must be absolute URLs. Its Rust code blocks run as doctests of the unpublished `readme-tests` workspace member.
- **`conferencier/guide/`** holds the user guide in Markdown. `overview.md` is the crate-level documentation; the other pages become modules of `conferencier::guide` (see `conferencier/src/guide.rs`) and are listed in `index.md`. A new page needs both. The guide is part of the package, so its Rust code blocks run as doctests and its intra-doc links are checked by `cargo doc`; link between pages and to API items with intra-doc links, not with file paths.
- **Migration steps** for every release belong to `conferencier/guide/migration.md`; `CHANGELOG.md` only links to them.
- **`docs/`** contains the [backlog](docs/BACKLOG.md) and the design records (RFCs and implementation reports) grouped by version series, such as `docs/devel-0.0.x/`.

## Reporting issues

- Use the issue tracker to describe bugs, desired features, or documentation gaps.
- Include reproduction steps, expected vs. actual behavior, and environment details when possible.
- Do not report security vulnerabilities in public issues; follow the [security policy](SECURITY.md) instead.

## Code of conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). Be respectful and collaborative. Assume good intent, provide constructive feedback, and help keep the project welcoming for everyone.

Happy hacking!
