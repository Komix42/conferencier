#[derive(conferencier_derive::ConfigTable)]
#[confer(rename = "Other")]
struct StructAttribute {
    value: i32,
}

#[derive(conferencier_derive::ConfigTable)]
enum Enum {
    A,
}

#[derive(conferencier_derive::ConfigTable)]
struct Tuple(i32);

#[derive(conferencier_derive::ConfigTable)]
struct Unit;

#[derive(conferencier_derive::ConfigTable)]
struct FieldAttribute {
    #[confer(flatten)]
    value: i32,
}

fn main() {}
