#[derive(conferencier_derive::ConfigTable)]
#[confer(section = "Unused")]
struct TableOnly {
    value: i32,
}

#[derive(conferencier_derive::ConfigTable)]
#[cfg_attr(any(), derive(conferencier_derive::ConfigScope))]
#[confer(section = "Disabled")]
struct DisabledScope {
    value: i32,
}

#[derive(conferencier_derive::ConfigTable)]
#[confer(section = "Manual")]
struct ManualScope {
    value: i32,
}

impl conferencier::ConfigScope for ManualScope {
    fn from_store(
        _: &conferencier::ConfigStore,
    ) -> conferencier::Result<conferencier::SharedScope<Self>> {
        unimplemented!()
    }

    async fn load(
        _: &conferencier::SharedScope<Self>,
        _: &conferencier::ConfigStore,
    ) -> conferencier::Result<()> {
        unimplemented!()
    }

    async fn save(
        _: &conferencier::SharedScope<Self>,
        _: &conferencier::ConfigStore,
    ) -> conferencier::Result<()> {
        unimplemented!()
    }
}

fn main() {}
