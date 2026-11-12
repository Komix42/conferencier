// `Datetime` has no `Default` value; loaded fields of that type need none.
#[derive(conferencier_derive::ConfigScope)]
#[confer(section = "Clock")]
struct Clock {
    started: conferencier::Datetime,
    stopped: Option<conferencier::Datetime>,
    laps: Vec<conferencier::Datetime>,
}

fn main() {}
