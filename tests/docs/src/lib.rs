//! Runs documentation examples that cannot be doctested in their own package.
//!
//! The repository `README.md` is also the `conferencier` package README on
//! crates.io, but Cargo copies it into the package root, so no `include_str!`
//! path works both in this workspace and in the published package. The examples
//! of `conferencier-derive` need `conferencier`, which the derive package cannot
//! depend on. This crate is not published.

#![doc(test(attr(deny(warnings))))]

#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
struct ReadmeDoctests;

#[cfg(doctest)]
#[doc = include_str!("../../../conferencier-derive/doc/config_scope_example.md")]
struct ConfigScopeExample;

#[cfg(doctest)]
#[doc = include_str!("../../../conferencier-derive/doc/config_table_example.md")]
struct ConfigTableExample;
