use std::marker::PhantomData;

use conferencier::ConfigTable;

#[allow(dead_code)]
#[derive(conferencier_derive::ConfigTable)]
struct Wrapper<T, U>
where
    T: ConfigTable + Default,
    U: ConfigTable,
{
    #[confer(table, default = T::default())]
    inner: T,
    #[confer(table, default = Vec::<U>::new())]
    items: Vec<U>,
    #[confer(ignore = Vec::<T>::new())]
    cache: Vec<T>,
    #[confer(ignore = PhantomData)]
    marker: PhantomData<U>,
}

#[allow(dead_code)]
#[derive(Default, conferencier_derive::ConfigTable)]
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
