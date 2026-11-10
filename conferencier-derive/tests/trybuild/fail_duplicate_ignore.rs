#[derive(conferencier_derive::ConferModule)]
struct TwiceBare {
    #[confer(ignore, ignore)]
    value: i32,
}

#[derive(conferencier_derive::ConferModule)]
struct BareThenExpression {
    #[confer(ignore)]
    #[confer(ignore = 5)]
    value: i32,
}

fn main() {}
