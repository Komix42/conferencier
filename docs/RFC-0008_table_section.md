# RFC-0008: Restrict `section` to derived scopes

Date: 2026-11-13
Status: Implemented
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.0.5
Amends: [RFC-0004](./RFC-0004_nested_tables.md)

## Decision

The `#[confer(section = "...")]` struct attribute selects the top-level section
of a derived `ConfigScope`. A nested `ConfigTable` has no section of its own:
its key comes from the containing field's name or `rename`.

- A struct deriving `ConfigScope` may use `section`.
- A struct deriving both `ConfigScope` and `ConfigTable` may use `section`;
  it affects only the scope. Derive order, aliases and separate derive
  attributes do not change this rule.
- A struct deriving only `ConfigTable` with `section` fails to compile. Remove
  the attribute, or derive `ConfigScope` too if the type is also a scope.

This amends RFC-0004's unconditional acceptance of `section` on a table derive.
It is a breaking change from 0.0.4, where that attribute was silently ignored
on a table-only derive.

## Implementation and tests

The scope derive emits a hidden marker implementation when `section` is present.
The table derive checks that marker rather than inspecting derive names or order;
manually implementing `ConfigScope` alone does not consume the derive attribute.
Compile tests cover table-only rejection, scope-only acceptance, both derive
orders, aliases, separate derive attributes, conditional derives and generic types.
