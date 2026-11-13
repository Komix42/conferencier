use std::collections::HashMap;

// Implements neither `Default` nor a shape supported by the derive.
#[allow(dead_code)]
struct Pool {
    size: usize,
}

impl Pool {
    fn with_size(size: usize) -> Self {
        Pool { size }
    }
}

#[allow(dead_code)]
#[derive(conferencier_derive::ConfigScope)]
struct Workers {
    threads: u32,
    #[confer(ignore = Pool::with_size(4))]
    pool: Pool,
    #[confer(ignore = HashMap::new())]
    index: HashMap<String, usize>,
    #[confer(ignore)]
    counter: u64,
}

fn main() {}
