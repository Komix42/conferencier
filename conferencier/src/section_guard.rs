//! Utilities for reconciling module-owned sections within the configuration store.

use std::collections::BTreeSet;

#[allow(dead_code)]
/// Tracks the set of keys owned by a module within a TOML section.
pub struct SectionGuard {
    known_keys: BTreeSet<String>,
}

#[allow(dead_code)]
impl SectionGuard {
    /// Creates a guard from an iterator of key names.
    pub fn new<I>(keys: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        let known_keys = keys.into_iter().map(Into::into).collect();
        Self { known_keys }
    }

    /// Returns the keys known to belong to the guarded section.
    pub fn known_keys(&self) -> &BTreeSet<String> {
        &self.known_keys
    }
}
