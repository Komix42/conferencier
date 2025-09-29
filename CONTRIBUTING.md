# Contributing

Thanks for your interest in improving **conferencier**! This quick guide explains how to get set up and submit changes.

## Development workflow

1. **Clone** the repository and install the latest stable Rust toolchain (the project targets the 2024 edition).
2. **Run tests** locally before opening a pull request:

   ```powershell
   cargo test --all-features
   ```

3. **Check formatting and lints**:

   ```powershell
   cargo fmt --all
   cargo clippy --all-targets --all-features
   ```

4. **Document your changes** by updating `CHANGELOG.md` and adding rustdoc comments or extra examples when appropriate.
5. **Open a pull request** with a clear description of the change, including any follow-up work or known limitations.

## Reporting issues

- Use the issue tracker to describe bugs, desired features, or documentation gaps.
- Include reproduction steps, expected vs. actual behavior, and environment details when possible.

## Code of conduct

Be respectful and collaborative. Assume good intent, provide constructive feedback, and help keep the project welcoming for everyone.

Happy hacking!
