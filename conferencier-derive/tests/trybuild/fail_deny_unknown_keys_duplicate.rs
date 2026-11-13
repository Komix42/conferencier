use conferencier_derive::ConfigScope;

#[derive(ConfigScope)]
#[confer(deny_unknown_keys)]
#[confer(deny_unknown_keys)]
struct Duplicate {
    value: String,
}

fn main() {}
