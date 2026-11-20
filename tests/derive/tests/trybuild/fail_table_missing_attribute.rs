#[derive(conferencier_derive::ConfigTable)]
struct Server {
    port: u16,
}

#[derive(conferencier_derive::ConfigScope)]
struct App {
    server: Server,
}

fn main() {}
