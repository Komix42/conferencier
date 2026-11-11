async fn make() -> Vec<u8> {
    Vec::new()
}

#[derive(conferencier_derive::ConferModule)]
struct UsesAwait {
    #[confer(ignore = make().await)]
    buffer: Vec<u8>,
}

fn main() {}
