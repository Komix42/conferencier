use conferencier_derive::{ConfigScope, ConfigTable};

#[derive(ConfigScope)]
#[confer(deny_unknown_keys)]
struct Scope {
    value: String,
}

#[derive(ConfigTable)]
#[confer(deny_unknown_keys)]
struct Table {
    value: String,
}

#[derive(ConfigScope, ConfigTable)]
#[confer(deny_unknown_keys)]
struct Both {
    value: String,
}

fn main() {}
