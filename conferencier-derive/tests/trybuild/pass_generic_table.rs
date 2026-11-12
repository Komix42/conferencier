use std::marker::PhantomData;

use conferencier::ConfigTable;

#[allow(dead_code)]
#[derive(conferencier_derive::ConfigTable)]
struct Wrapper<T, U>
where
    T: ConfigTable,
    U: ConfigTable,
{
    #[confer(table)]
    inner: T,
    #[confer(table)]
    items: Vec<U>,
    #[confer(ignore = Vec::<T>::new())]
    cache: Vec<T>,
    #[confer(ignore = PhantomData)]
    marker: PhantomData<U>,
}

#[allow(dead_code)]
#[derive(conferencier_derive::ConfigTable)]
struct Leaf {
    value: i64,
}

#[allow(dead_code)]
#[derive(conferencier_derive::ConfigScope)]
struct App {
    #[confer(table)]
    wrapped: Wrapper<Leaf, Leaf>,
}

fn main() {}
