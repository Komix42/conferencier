#[derive(conferencier_derive::ConferModule)]
struct ConflictingAttrs {
    #[confer(default = 5, init = "Default::default()")]
    value: i32,
}

fn main() {}
