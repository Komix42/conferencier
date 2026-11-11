#[derive(conferencier_derive::ConferTable)]
struct Inner {
    value: i32,
}

#[derive(conferencier_derive::ConferModule)]
struct TableWithDefault {
    #[confer(table, default = 5)]
    inner: Inner,
}

#[derive(conferencier_derive::ConferModule)]
struct TableWithIgnore {
    #[confer(table)]
    #[confer(ignore)]
    inner: Inner,
}

#[derive(conferencier_derive::ConferTable)]
struct DuplicateTable {
    #[confer(table, table)]
    inner: Inner,
}

fn main() {}
