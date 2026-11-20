use conferencier_derive::ConfigScope;

#[derive(ConfigScope)]
struct FieldAttribute {
    #[confer(deny_unknown_keys)]
    value: String,
}

fn main() {}
