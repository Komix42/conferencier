fn read_port() -> Result<u16, ()> {
    Ok(80)
}

#[derive(conferencier_derive::ConfigScope)]
struct WrongTypes {
    #[confer(default = "localhost")]
    host: String,
    #[confer(default = 1)]
    ratio: f64,
    #[confer(default = [1, 2])]
    ports: Vec<u16>,
}

#[derive(conferencier_derive::ConfigScope)]
struct InvalidControlFlow {
    #[confer(default = read_port()?)]
    port: u16,
    #[confer(default = async { 1_u16 }.await)]
    asynchronous: u16,
    #[confer(default = store)]
    hidden_store: u16,
    #[confer(default = scope)]
    hidden_scope: u16,
    #[confer(default = __confer_internal_store)]
    hidden_parameter: u16,
    #[confer(default = __confer_field_port)]
    hidden_field: u16,
}

#[derive(conferencier_derive::ConfigTable)]
struct NoDefault {
    port: u16,
}

#[derive(conferencier_derive::ConfigScope)]
struct NeedsDefault {
    #[confer(table, default)]
    child: NoDefault,
}

fn main() {}
