use std::marker::PhantomData;

use conferencier::ConfigScope;

#[allow(dead_code)]
#[derive(conferencier_derive::ConfigScope)]
struct Tagged<T> {
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
