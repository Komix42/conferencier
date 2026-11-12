#[derive(conferencier_derive::ConfigTable)]
struct Inner {
    value: i32,
}

#[derive(conferencier_derive::ConfigScope)]
struct OptionVec {
    #[confer(table)]
    inner: Option<Vec<Inner>>,
}

#[derive(conferencier_derive::ConfigScope)]
struct VecOption {
    #[confer(table)]
    inner: Vec<Option<Inner>>,
}

#[derive(conferencier_derive::ConfigTable)]
struct OptionOption {
    #[confer(table)]
    inner: Option<Option<Inner>>,
}

#[derive(conferencier_derive::ConfigTable)]
struct VecVec {
    #[confer(table)]
    inner: Vec<Vec<Inner>>,
}

#[derive(conferencier_derive::ConfigTable)]
struct Reference {
    #[confer(table)]
    inner: &'static Inner,
}

#[derive(conferencier_derive::ConfigTable)]
struct Tuple {
    #[confer(table)]
    inner: (Inner, Inner),
}

fn main() {}
