#[derive(conferencier_derive::ConferModule)]
#[confer(section = "Auth")]
struct AuthConfig {
    #[confer(default = "admin")]
    user: String,
    #[confer(default = 5)]
    retries: u8,
    #[confer(rename = "timeout_secs", default = 30)]
    timeout: u64,
    #[confer(default = ["alpha", "beta"])]
    roles: Vec<String>,
    #[confer(default = 3.5)]
    ratio: f32,
    #[confer(default = "2024-02-03T00:00:00Z")]
    started: toml::value::Datetime,
    optional: Option<bool>,
    #[confer(default = [1, 2, 3])]
    optional_numbers: Option<Vec<i32>>,
}

fn main() {}
