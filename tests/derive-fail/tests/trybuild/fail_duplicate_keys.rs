#[derive(conferencier_derive::ConfigScope)]
struct DuplicateKeys {
    one: i32,
    #[confer(rename = "one")]
    two: i32,
}

fn main() {}
