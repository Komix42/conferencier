use conferencier_derive::{ConfigScope, ConfigTable};

#[derive(ConfigScope)]
#[confer(deny_unknown_keys)]
struct Scope {
    value: String,
}

#[derive(ConfigTable)]
#[confer(deny_unknown_keys,)]
struct Table {
    value: String,
}

#[derive(ConfigScope, ConfigTable)]
#[confer(deny_unknown_keys, section = "Both",)]
struct Both {
    value: String,
}

#[derive(ConfigScope)]
#[confer(section = "SectionFirst", deny_unknown_keys,)]
struct SectionFirst {
    value: String,
}

#[derive(ConfigScope)]
#[confer(deny_unknown_keys, section = "StrictFirst")]
struct StrictFirst {
    value: String,
}

fn main() {}
