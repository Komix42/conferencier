use conferencier_derive::{ConfigScope, ConfigTable};

#[derive(ConfigScope)]
#[confer(section = "app")]
struct App {
    port: u16,
}

#[derive(ConfigTable)]
struct Endpoint {
    host: String,
}

fn main() {}
