#[allow(non_camel_case_types)]
struct String;
#[allow(non_camel_case_types)]
struct Vec;
struct Ok;
struct Err;

#[allow(non_camel_case_types)]
mod f32 {}
#[allow(non_camel_case_types)]
mod f64 {}
#[allow(non_camel_case_types)]
mod i64 {}

// Generated paths to the core crate must not resolve to this module.
#[allow(dead_code)]
mod conferencier {}

#[allow(unused_macros)]
macro_rules! format {
    ($($tokens:tt)*) => {
        ()
    };
}

#[allow(unused_macros)]
macro_rules! stringify {
    ($($tokens:tt)*) => {
        ()
    };
}

#[derive(conferencier_derive::ConfigTable)]
struct Nested {
    name: ::std::string::String,
    ratio: ::core::primitive::f32,
    values: ::std::vec::Vec<::core::primitive::i16>,
}

#[derive(conferencier_derive::ConfigScope)]
struct Hygienic {
    name: ::std::string::String,
    count: ::core::primitive::i32,
    ratio: ::core::primitive::f32,
    values: ::std::vec::Vec<::core::primitive::i32>,
    wide: ::core::primitive::u64,
    #[confer(table)]
    nested: Nested,
}

fn main() {}
