# Local CI on the "fili" workstation

Personal notes for running the full CI suite locally before pushing. They
describe one development machine, not a general contributor setup.

## Environment

- Windows checkout: `C:\Users\komix\source\conferencier`.
- Toolchains: stable and MSRV 1.85 (`cargo +1.85`), plus `cargo-audit`.
- Linux: WSL 2 distribution `Ubuntu-24.04`, regular user (not root), with
  stable, 1.85 and `cargo-audit` installed via rustup.
- Linux clone: `~/conferencier`, cloned from the Windows checkout so it also
  contains local, unpushed commits.
- macOS is not available locally; it is covered only by GitHub Actions.

## Commands

The suite mirrors the jobs in `.github/workflows/ci.yml`. Run the commands one
at a time from the repository root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo check -p conferencier --lib --locked
cargo check -p conferencier --lib --no-default-features --locked
cargo test --workspace --all-features --locked
cargo test -p conferencier --no-default-features --locked
cargo test -p conferencier --features toml/preserve_order --test scope_integration strict_scope_reports_smallest_unknown_key_with_preserved_order --locked -- --ignored
TRYBUILD=overwrite cargo test -p derive-fail-tests --locked -- --ignored
git checkout -- tests/derive-fail/tests/trybuild
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo semver-checks --package conferencier --baseline-rev v0.1.0-beta.1 --release-type minor --default-features
cargo semver-checks --package conferencier --baseline-rev v0.1.0-beta.1 --release-type minor --only-explicit-features
cargo +1.85 check --workspace --all-targets --all-features --locked
cargo +1.85 check -p conferencier --no-default-features --locked
cargo +1.85 test -p conferencier --all-features --locked
cargo +1.85 test -p derive-tests --locked
cargo +1.85 test -p derive-fail-tests --locked -- --ignored
cargo audit
cargo publish --workspace --dry-run --locked
cargo package -p conferencier -p conferencier-derive --no-verify --locked
bash tests/consumers/check.sh
```

- In PowerShell, set `TRYBUILD` and `RUSTDOCFLAGS` before their commands with
  `$env:NAME = "value"` and remove them afterwards with `Remove-Item Env:NAME`.
- The stable compile-fail check rewrites the snapshots that differ from Rust
  1.85; the following `git checkout` restores them. It also discards
  uncommitted snapshot changes, so run the suite on a committed tree.
- The semver commands require `cargo-semver-checks` on the machine running
  them (`cargo install cargo-semver-checks --locked` with stable Rust). Keep
  their baseline tag in sync with CI after each publication.
- The publish dry run needs network access and a clean working tree. If it
  reports stale sources of the unpublished derive crate, rerun it with a fresh
  temporary `CARGO_HOME`.
- The packaged-consumer script requires `jq` and `tar`. It packages both crates,
  builds temporary copies of the three downstream consumers against the archives
  on Rust 1.85, and removes those temporary copies afterwards.
- When CI changes, update this list.

### Fresh MSRV resolution

CI also checks a fresh, MSRV-aware dependency resolution on Linux. Run it in
the Linux clone; it uses the committed `HEAD` and leaves the workspace
`Cargo.lock` untouched:

```sh
(
  set -eu
  dir=$(mktemp -d)
  printf 'Fresh checkout: %s\n' "$dir"
  git archive HEAD | tar -x -C "$dir"
  rm "$dir/Cargo.lock"
  cd "$dir"
  cargo +1.85 check --workspace --all-targets --all-features
  cargo +1.85 check -p conferencier --no-default-features
)
```

The temporary directory can be removed after the check.

### Packaged doctests

CI also tests the guide doctests from the packaged crate, outside the workspace.
On Linux (including WSL), run the following from the repository root after the
publish dry run; this requires `jq` and `tar`:

```sh
(
  set -eu
  version=$(cargo metadata --no-deps --format-version 1 --locked | jq -er '.packages[] | select(.name == "conferencier") | .version')
  dir=$(mktemp -d)
  printf 'Package directory: %s\n' "$dir"
  cargo package -p conferencier -p conferencier-derive --no-verify --locked
  tar -xzf "target/package/conferencier-$version.crate" -C "$dir"
  tar -xzf "target/package/conferencier-derive-$version.crate" -C "$dir"
  cd "$dir/conferencier-$version"
  cargo test --doc --all-features --config "patch.crates-io.conferencier-derive.path='$dir/conferencier-derive-$version'"
)
```

The local derive patch changes the packaged lockfile, so this test cannot use
`--locked`. The temporary directory can be removed after the test.

## Windows

Run the commands in PowerShell in the Windows checkout. Unix-only tests, such
as the permission and special-file save tests, are skipped here.

## Linux (WSL)

Update the Linux clone, then run the same commands in it:

```sh
cd ~/conferencier && git fetch && git reset --hard origin/develop
```

Here `origin` is the Windows checkout, so the reset also follows amended local
history. It discards any changes made in the Linux clone.

From PowerShell, a script can be passed to the distribution on standard input;
strip carriage returns first:

```powershell
$script -replace "`r", "" | wsl -d Ubuntu-24.04 -- bash
```

Keep the clone on the Linux filesystem, not `/mnt/c/...`: Unix permissions
behave differently on Windows drives and the permission tests would fail
spuriously. Run as the regular user, because root bypasses permission checks.

## One-time WSL setup

In PowerShell:

```powershell
wsl --install -d Ubuntu-24.04
```

In Ubuntu:

```sh
sudo apt update && sudo apt install -y build-essential git curl pkg-config jq
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env
rustup toolchain install 1.85
cargo install cargo-audit --locked
cargo install cargo-semver-checks --locked
git clone -b develop /mnt/c/Users/komix/source/conferencier ~/conferencier
```
