#[derive(conferencier_derive::ConferModule)]
struct DuplicateKeys {
    one: i32,
    #[confer(rename = "one")]
    two: i32,
}

fn main() {}
