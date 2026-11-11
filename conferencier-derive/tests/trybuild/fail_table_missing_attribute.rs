#[derive(conferencier_derive::ConferTable)]
struct Server {
    port: u16,
}

#[derive(conferencier_derive::ConferModule)]
struct App {
    server: Server,
}

fn main() {}
