// An ignored field has no TOML key, so it may share its name with the key of another field,
// whichever of the two is declared first.
#[allow(dead_code)]
#[derive(conferencier_derive::ConferModule)]
struct Collision {
    #[confer(ignore)]
    id: u64,
    #[confer(rename = "id")]
    identifier: u64,
    #[confer(rename = "name")]
    title: String,
    #[confer(ignore)]
    name: String,
}

fn main() {}
