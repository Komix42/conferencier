#[derive(conferencier_derive::ConferModule)]
struct InvalidDatetime {
    #[confer(default = "tomorrow")]
    started: conferencier::Datetime,
}

fn main() {}
