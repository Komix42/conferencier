How to upgrade code and configuration files from earlier releases.

## From 0.0.4 to 0.0.5

Migrating from 0.0.4: the feature formerly named `with-derive` is now `derive`. The repository's `CHANGELOG.md` lists the API and signature changes.

## From 0.0.3 to 0.0.4

Migrating from 0.0.3: `#[confer(init = ...)]` was removed. Replace `ignore, init = "<expr>"` and `ignore, default = <literal>` with `ignore = <expr>` (no quotes), drop `rename` from ignored fields, and delete `init` from loaded fields (add `default` only if a missing key should fall back to a value). The repository's `CHANGELOG.md` has the full table.
