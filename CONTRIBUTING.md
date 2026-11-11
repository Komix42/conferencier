# Contributing

Thanks for your interest in improving **conferencier**! This quick guide explains how to get set up and submit changes.

## Development workflow

1. **Clone** the repository and install the latest stable Rust toolchain (the project targets the 2024 edition; the minimum supported Rust version is 1.85, so language features and standard library APIs stabilized later, such as let chains, cannot be used).
2. **Run tests** locally before opening a pull request:

   ```powershell
   cargo test --workspace --all-features
   cargo test -p conferencier --no-default-features
   ```

   The first command covers both crates, including the compile-fail tests of the derive macro. Their expected compiler output (`conferencier-derive/tests/trybuild/*.stderr`) follows the latest stable Rust. If you change a diagnostic on purpose, rerun the tests with the `TRYBUILD=overwrite` environment variable set and review the updated files.

3. **Check formatting, lints and docs**:

   ```powershell
   cargo fmt --all
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo doc --workspace --no-deps
   ```

   CI runs the same checks on Linux, Windows and macOS, checks the minimum supported Rust version and treats rustdoc warnings as errors.

4. **Document your changes** by updating `CHANGELOG.md` and adding rustdoc comments or extra examples when appropriate.
5. **Open a pull request** against the `develop` branch with a clear description of the change, including any follow-up work or known limitations.

## Reporting issues

- Use the issue tracker to describe bugs, desired features, or documentation gaps.
- Include reproduction steps, expected vs. actual behavior, and environment details when possible.
- Do not report security vulnerabilities in public issues; follow the [security policy](SECURITY.md) instead.

## Code of conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). Be respectful and collaborative. Assume good intent, provide constructive feedback, and help keep the project welcoming for everyone.

Happy hacking!
