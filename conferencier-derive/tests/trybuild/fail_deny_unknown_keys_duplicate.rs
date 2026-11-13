use conferencier_derive::ConfigScope;

#[derive(ConfigScope)]
#[confer(deny_unknown_keys)]
#[confer(deny_unknown_keys)]
struct Duplicate {
    value: String,
}

#[derive(ConfigScope)]
#[confer(deny_unknown_keys, deny_unknown_keys)]
struct DuplicateInList {
    value: String,
}

fn main() {}
