type Port = u16;

#[derive(conferencier_derive::ConfigScope)]
struct AliasField {
    port: Port,
}

fn main() {}
