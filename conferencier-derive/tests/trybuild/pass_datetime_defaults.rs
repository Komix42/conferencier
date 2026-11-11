// Shadows the `toml` crate: generated code must not depend on `toml` paths,
// so users don't need `toml` as a direct dependency.
mod toml {}

#[derive(conferencier_derive::ConferModule)]
#[confer(section = "Schedule")]
struct Schedule {
    #[confer(default = "2024-02-03T00:00:00Z")]
    started: conferencier::Datetime,
    #[confer(default = "07:30:00")]
    alarm: Option<conferencier::Datetime>,
    #[confer(default = ["2024-01-01", "2024-12-31"])]
    holidays: Vec<conferencier::Datetime>,
    #[confer(default = ["1979-05-27T07:32:00-08:00"])]
    milestones: Option<Vec<conferencier::Datetime>>,
}

fn main() {}
