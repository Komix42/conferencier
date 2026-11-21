// Initializers cannot see the parameters or the locals of the generated `from_table`.
#[derive(conferencier_derive::ConfigTable)]
struct UsesTable {
    port: u16,
    #[confer(ignore = table.len())]
    size: usize,
}

#[derive(conferencier_derive::ConfigTable)]
struct UsesPath {
    port: u16,
    #[confer(ignore = path.to_string())]
    location: String,
}

#[derive(conferencier_derive::ConfigTable)]
struct UsesLocal {
    port: u16,
    #[confer(ignore = __confer_field_port)]
    copy: u16,
}

fn main() {}
