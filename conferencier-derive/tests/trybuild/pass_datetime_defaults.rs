// Shadows the `toml` crate: generated code must not depend on `toml` paths,
// so users don't need `toml` as a direct dependency.
mod toml {}

#[derive(conferencier_derive::ConfigScope)]
#[confer(section = "Schedule")]
struct Schedule {
    #[confer(default = "2024-02-03T00:00:00Z".parse().unwrap())]
    started: conferencier::Datetime,
    #[confer(default = "07:30:00".parse().unwrap())]
    alarm: conferencier::Datetime,
    #[confer(default = vec!["2024-01-01".parse().unwrap(), "2024-12-31".parse().unwrap()])]
    holidays: Vec<conferencier::Datetime>,
    #[confer(default = vec!["1979-05-27T07:32:00-08:00".parse().unwrap()])]
    milestones: Vec<conferencier::Datetime>,
}

fn main() {}
