// `?` stays inside the initializer closure and cannot return an error from `from_confer`.
#[derive(conferencier_derive::ConferModule)]
struct UsesTry {
    #[confer(ignore = "5".parse::<u8>()?)]
    value: u8,
}

fn main() {}
