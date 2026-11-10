use std::marker::PhantomData;

#[allow(dead_code)]
#[derive(conferencier_derive::ConferModule)]
struct Tagged<T: Send + Sync + 'static> {
    port: u16,
    #[confer(ignore = Vec::<T>::new())]
    items: Vec<T>,
    #[confer(ignore = PhantomData)]
    marker: PhantomData<T>,
}

fn main() {}
