use std::marker::PhantomData;

use conferencier::ConfigScope;

// Explicit user bounds, as 0.1.0-beta.1 required them, must keep compiling.
#[allow(dead_code)]
#[derive(conferencier_derive::ConfigScope)]
struct Tagged<T: Send + Sync + 'static> {
    port: u16,
    #[confer(ignore = Vec::<T>::new())]
    items: Vec<T>,
    #[confer(ignore = PhantomData)]
    marker: PhantomData<T>,
}

fn assert_config_scope<T: ConfigScope>() {}

fn main() {
    assert_config_scope::<Tagged<u8>>();
}
