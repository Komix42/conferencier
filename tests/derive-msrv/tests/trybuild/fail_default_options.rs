#[derive(conferencier_derive::ConfigTable)]
struct Child {}

#[derive(conferencier_derive::ConfigScope)]
struct PlainOption {
    #[confer(default)]
    plain: Option<u16>,
}

#[derive(conferencier_derive::ConfigScope)]
struct StdOption {
    #[confer(default = Some(1))]
    qualified: std::option::Option<u16>,
}

#[derive(conferencier_derive::ConfigScope)]
struct CoreOptionVec {
    #[confer(default = None)]
    vector: core::option::Option<Vec<u16>>,
}

#[derive(conferencier_derive::ConfigScope)]
struct OptionalTable {
    #[confer(table, default)]
    child: Option<Child>,
}

#[derive(conferencier_derive::ConfigScope)]
struct StdOptionalTable {
    #[confer(table, default = None)]
    child: std::option::Option<Child>,
}

#[derive(conferencier_derive::ConfigScope)]
struct CoreOptionalTable {
    #[confer(table, default)]
    child: core::option::Option<Child>,
}

#[derive(conferencier_derive::ConfigScope)]
struct DuplicateInList {
    #[confer(default, default = 1)]
    port: u16,
}

#[derive(conferencier_derive::ConfigTable)]
struct DuplicateAcrossAttributes {
    #[confer(default = 1)]
    #[confer(default)]
    port: u16,
}

fn main() {}
