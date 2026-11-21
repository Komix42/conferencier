//! Generated code must not trigger clippy lints in the user's crate; checked by
//! `cargo clippy --workspace --all-targets`. Nursery changes often, so only its lints
//! that were hit before are denied.
#![deny(clippy::pedantic, clippy::significant_drop_tightening)]

use conferencier::{ConfigScope, ConfigTable};

#[derive(ConfigScope)]
#[confer(section = "loaded")]
pub struct Loaded {
    pub port: u16,
    pub name: String,
    #[confer(table)]
    pub inner: Inner,
}

#[derive(ConfigScope)]
#[confer(section = "only_ignored")]
pub struct OnlyIgnored {
    #[confer(ignore)]
    pub counter: u64,
}

#[derive(ConfigScope)]
#[confer(section = "generic", deny_unknown_keys)]
pub struct Generic<T: Send + Sync + 'static> {
    pub port: u16,
    #[confer(ignore)]
    pub marker: Option<T>,
}

#[derive(ConfigTable, Default)]
pub struct Inner {
    pub enabled: bool,
}

#[test]
fn derived_types_compile() {
    let store = conferencier::ConfigStore::default();
    // Only compilation matters here; the empty store lacks the required keys.
    let _ = Loaded::from_store(&store);
    let _ = Generic::<u8>::from_store(&store);
    assert!(OnlyIgnored::from_store(&store).is_ok());
}
