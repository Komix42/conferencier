#[derive(conferencier_derive::ConfigTable)]
struct Inner {
    value: i32,
}

#[derive(conferencier_derive::ConfigScope)]
struct TableWithIgnore {
    #[confer(table)]
    #[confer(ignore)]
    inner: Inner,
}

#[derive(conferencier_derive::ConfigTable)]
struct DuplicateTable {
    #[confer(table, table)]
    inner: Inner,
}

fn main() {}
