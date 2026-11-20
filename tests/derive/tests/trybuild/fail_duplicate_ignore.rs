#[derive(conferencier_derive::ConfigScope)]
struct TwiceBare {
    #[confer(ignore, ignore)]
    value: i32,
}

#[derive(conferencier_derive::ConfigScope)]
struct BareThenExpression {
    #[confer(ignore)]
    #[confer(ignore = 5)]
    value: i32,
}

fn main() {}
