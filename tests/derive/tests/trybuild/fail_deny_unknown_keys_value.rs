use conferencier_derive::ConfigScope;

#[derive(ConfigScope)]
#[confer(deny_unknown_keys = true)]
struct WithValue {
    value: String,
}

#[derive(ConfigScope)]
#[confer(deny_unknown_keys(true))]
struct WithArguments {
    value: String,
}

fn main() {}
