// RFC-0005 N7: the migration guard applies only to ConfigScope; a ConfigTable has no section.
use conferencier::ConfigTable;

#[allow(dead_code)]
#[derive(conferencier_derive::ConfigTable)]
struct ConferLimits {
    burst: u32,
}

fn main() {
    let mut table = conferencier::toml::Table::new();
    table.insert("burst".into(), 5.into());
    assert_eq!(ConferLimits::from_table("App.limits", table).unwrap().burst, 5);
}