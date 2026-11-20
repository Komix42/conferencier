#[derive(conferencier_derive::ConfigTable)]
struct Child {
    value: u16,
}

#[derive(conferencier_derive::ConfigScope)]
struct NoTable {
    #[confer(field_defaults)]
    value: u16,
}

#[derive(conferencier_derive::ConfigScope)]
struct OptionalTable {
    #[confer(table, field_defaults)]
    child: Option<Child>,
}

#[derive(conferencier_derive::ConfigTable)]
struct QualifiedOptionalTable {
    #[confer(table, field_defaults)]
    child: core::option::Option<Child>,
}

#[derive(conferencier_derive::ConfigTable)]
struct TableArray {
    #[confer(table, field_defaults)]
    children: Vec<Child>,
}

#[derive(conferencier_derive::ConfigScope)]
struct DefaultConflict {
    #[confer(table, field_defaults, default)]
    child: Child,
}

#[derive(conferencier_derive::ConfigTable)]
struct ExpressionConflict {
    #[confer(default = Child { value: 1 })]
    #[confer(field_defaults, table)]
    child: Child,
}

#[derive(conferencier_derive::ConfigScope)]
struct DuplicateInList {
    #[confer(table, field_defaults, field_defaults)]
    child: Child,
}

#[derive(conferencier_derive::ConfigTable)]
struct DuplicateAcrossAttributes {
    #[confer(table, field_defaults)]
    #[confer(field_defaults)]
    child: Child,
}

#[derive(conferencier_derive::ConfigScope)]
struct AssignedValue {
    #[confer(table, field_defaults = true)]
    child: Child,
}

#[derive(conferencier_derive::ConfigTable)]
struct ParenthesizedValue {
    #[confer(table, field_defaults(true))]
    child: Child,
}

#[derive(conferencier_derive::ConfigScope)]
struct TableAndIgnore {
    #[confer(table, field_defaults, ignore)]
    child: Child,
}

type MaybeChild = Option<Child>;

#[derive(conferencier_derive::ConfigTable)]
struct OpaqueOption {
    #[confer(table, field_defaults)]
    child: MaybeChild,
}

fn main() {}
