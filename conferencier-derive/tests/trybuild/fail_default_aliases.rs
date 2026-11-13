type MaybePort = Option<u16>;

#[derive(conferencier_derive::ConfigScope)]
struct ScalarAlias {
    #[confer(default)]
    port: MaybePort,
}

#[derive(conferencier_derive::ConfigTable)]
struct Child {}

type MaybeChild = Option<Child>;

#[derive(conferencier_derive::ConfigScope)]
struct TableAlias {
    #[confer(table, default)]
    child: MaybeChild,
}

fn main() {}
