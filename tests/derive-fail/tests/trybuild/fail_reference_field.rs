#[derive(conferencier_derive::ConfigScope)]
struct Borrowed {
    value: &'static str,
}

fn main() {}
