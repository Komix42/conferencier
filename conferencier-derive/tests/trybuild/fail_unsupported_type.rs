use std::collections::HashMap;

#[derive(conferencier_derive::ConferModule)]
struct BadType {
    map: HashMap<String, String>,
}

fn main() {}
