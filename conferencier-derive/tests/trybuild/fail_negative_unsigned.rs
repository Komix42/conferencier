#[derive(conferencier_derive::ConferModule)]
struct NegativeUnsigned {
    #[confer(default = -1)]
    retries: u32,
}

fn main() {}
