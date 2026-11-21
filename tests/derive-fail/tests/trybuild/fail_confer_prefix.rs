// RFC-0005 N7: until 0.2.0, a ConfigScope without `section` whose name 0.0.4 shortened
// (`Confer` followed by an uppercase letter) must choose its section explicitly.
#[derive(conferencier_derive::ConfigScope)]
struct ConferApp {
    port: u16,
}

#[derive(conferencier_derive::ConfigScope)]
struct r#ConferRaw {
    port: u16,
}

#[derive(conferencier_derive::ConfigScope)]
struct ConferConferApp {
    port: u16,
}

#[derive(conferencier_derive::ConfigScope)]
struct ConferÉtat {
    port: u16,
}

// A struct deriving both traits is guarded by ConfigScope.
#[derive(conferencier_derive::ConfigScope, conferencier_derive::ConfigTable)]
struct ConferBoth {
    port: u16,
}

fn main() {}