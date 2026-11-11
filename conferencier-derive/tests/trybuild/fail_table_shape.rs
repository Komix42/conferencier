#[derive(conferencier_derive::ConferTable)]
struct Inner {
    value: i32,
}

#[derive(conferencier_derive::ConferModule)]
struct OptionVec {
    #[confer(table)]
    inner: Option<Vec<Inner>>,
}

#[derive(conferencier_derive::ConferModule)]
struct VecOption {
    #[confer(table)]
    inner: Vec<Option<Inner>>,
}

#[derive(conferencier_derive::ConferTable)]
struct OptionOption {
    #[confer(table)]
    inner: Option<Option<Inner>>,
}

#[derive(conferencier_derive::ConferTable)]
struct VecVec {
    #[confer(table)]
    inner: Vec<Vec<Inner>>,
}

#[derive(conferencier_derive::ConferTable)]
struct Reference {
    #[confer(table)]
    inner: &'static Inner,
}

#[derive(conferencier_derive::ConferTable)]
struct Tuple {
    #[confer(table)]
    inner: (Inner, Inner),
}

fn main() {}
