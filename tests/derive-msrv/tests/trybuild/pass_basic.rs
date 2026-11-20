#[derive(conferencier_derive::ConfigScope)]
#[confer(section = "Auth")]
struct AuthConfig {
    #[confer(default = "admin".into())]
    user: String,
    #[confer(default = 5)]
    retries: u8,
    #[confer(rename = "timeout_secs", default = 30)]
    timeout: u64,
    #[confer(default = vec!["alpha".into(), "beta".into()])]
    roles: Vec<String>,
    #[confer(default = 3.5)]
    ratio: f32,
    #[confer(default = "2024-02-03T00:00:00Z".parse().unwrap())]
    started: conferencier::Datetime,
    optional: Option<bool>,
    optional_numbers: Option<Vec<i32>>,
}

struct Default;

#[derive(conferencier_derive::ConfigScope)]
struct BareDefaults {
    #[confer(default, rename = "text",)]
    text: String,
    #[confer(default)]
    numbers: Vec<u16>,
}

fn main() {}
