use std::collections::HashMap;

#[derive(conferencier_derive::ConfigScope)]
struct BadType {
    map: HashMap<String, String>,
}

fn main() {}
