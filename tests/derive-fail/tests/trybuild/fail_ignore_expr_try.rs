// `?` stays inside the initializer closure and cannot return an error from `from_store`.
#[derive(conferencier_derive::ConfigScope)]
struct UsesTry {
    #[confer(ignore = "5".parse::<u8>()?)]
    value: u8,
}

fn main() {}
