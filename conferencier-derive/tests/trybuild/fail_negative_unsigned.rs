#[derive(conferencier_derive::ConfigScope)]
struct NegativeUnsigned {
    #[confer(default = -1)]
    retries: u32,
}

fn main() {}
