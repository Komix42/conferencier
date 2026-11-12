use conferencier::ConfigTable;
use conferencier_derive::{ConfigScope, ConfigTable};

#[allow(dead_code)]
#[derive(ConfigTable)]
struct Limits {
    #[confer(default = 10)]
    burst: u32,
    rate: Option<f32>,
}

#[allow(dead_code)]
#[derive(ConfigTable)]
struct Endpoint {
    url: String,
    #[confer(table)]
    limits: Limits,
    #[confer(table, rename = "fallback-limits")]
    fallback: Option<Limits>,
    #[confer(ignore)]
    hits: u64,
}

#[allow(dead_code)]
#[derive(ConfigTable)]
struct Node {
    name: String,
    #[confer(table)]
    children: Vec<Node>,
}

#[allow(dead_code)]
#[derive(ConfigTable)]
struct Empty {}

// Saving these types cannot fail, so the generated `to_table` leaves its path unnamed.
#[allow(dead_code)]
#[derive(ConfigTable)]
struct Scalars {
    text: String,
    flag: bool,
    small: i8,
    medium: i32,
    signed: isize,
    unsigned: u32,
    ratio: f32,
    precise: f64,
    stamp: Option<toml::value::Datetime>,
    flags: Vec<bool>,
    ratios: Option<Vec<f32>>,
}

// One struct can be both a module and a table; `section` only affects the module.
#[allow(dead_code)]
#[derive(ConfigScope, ConfigTable)]
#[confer(section = "Shared")]
struct Shared {
    #[confer(default = true)]
    enabled: bool,
}

#[allow(dead_code)]
#[derive(ConfigScope)]
struct App {
    #[confer(table)]
    primary: Endpoint,
    #[confer(table)]
    backup: Option<Endpoint>,
    #[confer(table)]
    upstream: Vec<Endpoint>,
    #[confer(table)]
    tree: Node,
    #[confer(table)]
    empty: Empty,
    #[confer(table)]
    scalars: Scalars,
    #[confer(table)]
    shared: Shared,
    #[confer(table)]
    qualified: ::std::vec::Vec<Endpoint>,
}

fn main() {
    fn assert_table<T: ConfigTable>() {}
    assert_table::<Endpoint>();
    assert_table::<Shared>();
}
