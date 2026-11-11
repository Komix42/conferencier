// A string literal is an ordinary `&str` expression, not code to be parsed.
#[derive(conferencier_derive::ConferModule)]
struct WrongType {
    #[confer(ignore = "abc")]
    label: String,
}

fn main() {}
