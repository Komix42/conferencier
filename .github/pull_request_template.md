## Summary

<!-- What does this change do and why? Target `develop` for ordinary changes;
     release PRs target `main` from `develop`. -->

Closes #

## Checklist

- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` passes
- [ ] `cargo test --workspace --all-features --locked` passes
- [ ] `cargo test -p conferencier --no-default-features --locked` passes
- [ ] Tests cover the change (or it does not need them)
- [ ] Compatibility and MSRV impacts of public API, derive and dependency
      changes have been reviewed where relevant (see the [compatibility policy](https://github.com/Komix42/conferencier/blob/develop/CONTRIBUTING.md#01x-compatibility-and-release-checks))
- [ ] `CHANGELOG.md` is updated where needed (`[Unreleased]` for ordinary
      changes; the version heading for release PRs; breaking changes are marked
      with **Breaking:**)
- [ ] Documentation (rustdoc, README) is updated where needed

## Release PRs only

<!-- Remove this section for non-release PRs. -->

- [ ] Before merging, complete the pre-publication items in the
      [release checklist](https://github.com/Komix42/conferencier/blob/develop/CONTRIBUTING.md#release-checklist)
      and record packaged-consumer and example-application test results in this
      PR description
