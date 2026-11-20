//! Runs the Rust code blocks of the repository `README.md` as doctests.
//!
//! The README is also the `conferencier` package README on crates.io, but
//! Cargo copies it into the package root, so no `include_str!` path works both
//! in this workspace and in the published package. This crate is not published.

#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
struct ReadmeDoctests;
