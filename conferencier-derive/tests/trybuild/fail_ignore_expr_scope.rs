// Initializers cannot see the `store` argument or the locals of the generated `from_store`.
#[derive(conferencier_derive::ConfigScope)]
struct UsesStore {
    #[confer(ignore = store.clone())]
    handle: conferencier::ConfigStore,
}

#[derive(conferencier_derive::ConfigScope)]
struct UsesLocal {
    port: u16,
    #[confer(ignore = __confer_port)]
    copy: u16,
}

fn main() {}
