use std::marker::PhantomData;

use conferencier::ConferTable;

#[allow(dead_code)]
#[derive(conferencier_derive::ConferTable)]
struct Wrapper<T, U>
where
    T: ConferTable,
    U: ConferTable,
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
#[derive(conferencier_derive::ConferTable)]
struct Leaf {
    value: i64,
}

#[allow(dead_code)]
#[derive(conferencier_derive::ConferModule)]
struct App {
    #[confer(table)]
    wrapped: Wrapper<Leaf, Leaf>,
}

fn main() {}
