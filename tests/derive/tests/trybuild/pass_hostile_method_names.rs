#[allow(dead_code)]
trait HostileMethods {
    fn write(&self) -> u8 {
        0
    }

    fn read(&self) -> u8 {
        0
    }

    fn remove(&mut self, _: &str) -> Option<()> {
        None
    }

    fn insert(&mut self, _: String, _: ()) {}

    fn len(&self) {}

    fn into_iter(self)
    where
        Self: Sized,
    {
    }

    fn iter(&self) {}

    fn enumerate(self)
    where
        Self: Sized,
    {
    }

    fn push(&mut self, _: ()) {}

    fn set_section(&self, _: &str, _: ()) {}

    fn is_finite(&self) {}
}

impl<T> HostileMethods for T {}

#[allow(dead_code)]
#[derive(conferencier_derive::ConfigTable)]
struct Child {
    value: i32,
}

#[allow(dead_code)]
#[derive(conferencier_derive::ConfigScope)]
struct App {
    count: u8,
    numbers: Vec<i32>,
    ratios: Vec<f32>,
    #[confer(table)]
    child: Child,
    #[confer(table)]
    children: Vec<Child>,
}

fn main() {}
