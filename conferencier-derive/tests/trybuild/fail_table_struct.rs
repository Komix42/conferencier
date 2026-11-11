#[derive(conferencier_derive::ConferTable)]
#[confer(rename = "Other")]
struct StructAttribute {
    value: i32,
}

#[derive(conferencier_derive::ConferTable)]
enum Enum {
    A,
}

#[derive(conferencier_derive::ConferTable)]
struct Tuple(i32);

#[derive(conferencier_derive::ConferTable)]
struct Unit;

#[derive(conferencier_derive::ConferTable)]
struct FieldAttribute {
    #[confer(flatten)]
    value: i32,
}

fn main() {}
