#[derive(conferencier_derive::ConfigScope)]
struct InvalidDatetime {
    #[confer(default = "tomorrow")]
    started: conferencier::Datetime,
}

fn main() {}
