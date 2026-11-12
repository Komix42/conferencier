#[derive(conferencier_derive::ConfigScope)]
struct IgnoreWithDefault {
    #[confer(ignore, default = 5)]
    value: i32,
}

#[derive(conferencier_derive::ConfigScope)]
struct IgnoreWithRename {
    #[confer(rename = "other")]
    #[confer(ignore)]
    value: i32,
}

fn main() {}
