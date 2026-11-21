#[derive(conferencier_derive::ConfigScope)]
struct FieldTypo {
    #[confer(defualt = 1)]
    value: i32,
}

#[derive(conferencier_derive::ConfigScope)]
#[confer(sectoin = "App")]
struct StructTypo {
    value: i32,
}

fn main() {}
