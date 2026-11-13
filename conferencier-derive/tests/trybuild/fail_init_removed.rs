#[derive(conferencier_derive::ConfigScope)]
struct OnLoadedField {
    #[confer(init = "Default::default()")]
    value: i32,
}

#[derive(conferencier_derive::ConfigScope)]
struct OnIgnoredField {
    #[confer(ignore, init = "Vec::new()")]
    cache: Vec<u8>,
}

#[derive(conferencier_derive::ConfigScope)]
struct WithoutValue {
    #[confer(init)]
    value: i32,
}

fn main() {}
