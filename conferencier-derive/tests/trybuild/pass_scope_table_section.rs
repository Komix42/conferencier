use conferencier_derive::{ConfigScope as Scope, ConfigTable as Table};

#[derive(Scope)]
#[confer(section = "ScopeOnly")]
struct ScopeOnly {
    value: i32,
}

#[derive(Scope, Table)]
#[confer(section = "ScopeFirst")]
struct ScopeFirst {
    value: i32,
}

#[derive(Table, Scope)]
#[confer(section = "TableFirst")]
struct TableFirst {
    value: i32,
}

#[derive(conferencier_derive::ConfigTable)]
#[derive(conferencier_derive::ConfigScope)]
#[confer(section = "Separate")]
struct Separate {
    value: i32,
}

#[derive(Table)]
#[cfg_attr(all(), derive(Scope))]
#[confer(section = "Conditional")]
struct Conditional {
    value: i32,
}

#[derive(Table, Scope)]
#[confer(section = "Generic")]
struct Generic<T: Default + Send + Sync + 'static> {
    #[confer(ignore)]
    value: T,
}

fn main() {}
