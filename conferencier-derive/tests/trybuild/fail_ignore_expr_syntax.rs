#[derive(conferencier_derive::ConfigScope)]
struct MissingExpression {
    #[confer(ignore = )]
    value: i32,
}

fn main() {}
