// Initializers cannot see the `store` argument or the locals of the generated `from_confer`.
#[derive(conferencier_derive::ConferModule)]
struct UsesStore {
    #[confer(ignore = store.clone())]
    handle: conferencier::SharedConfer,
}

#[derive(conferencier_derive::ConferModule)]
struct UsesLocal {
    port: u16,
    #[confer(ignore = __confer_port)]
    copy: u16,
}

fn main() {}
