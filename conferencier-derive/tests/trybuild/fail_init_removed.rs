#[derive(conferencier_derive::ConferModule)]
struct OnLoadedField {
    #[confer(init = "Default::default()")]
    value: i32,
}

#[derive(conferencier_derive::ConferModule)]
struct OnIgnoredField {
    #[confer(ignore, init = "Vec::new()")]
    cache: Vec<u8>,
}

#[derive(conferencier_derive::ConferModule)]
struct WithoutValue {
    #[confer(init)]
    value: i32,
}

fn main() {}
