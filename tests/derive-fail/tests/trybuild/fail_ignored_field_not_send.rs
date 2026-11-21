use std::rc::Rc;

#[derive(conferencier_derive::ConfigScope)]
struct ScopeWithNonSendIgnoredField {
    #[confer(ignore)]
    value: Rc<i32>,
}

fn main() {}
