// The only case asserting the `on_unimplemented` trait error; its wording varies between rustc
// releases.
#[derive(conferencier_derive::ConfigTable)]
struct NotATable {
    #[confer(table)]
    name: String,
}

fn main() {}
