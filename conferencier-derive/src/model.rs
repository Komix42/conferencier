use proc_macro2::{Span, TokenStream};
use syn::Ident;

/// Intermediate representation of a module annotated with `#[derive(ConferModule)]`.
#[derive(Debug, Clone)]
pub struct Module {
    pub ident: Ident,
    pub generics: syn::Generics,
    pub section: String,
    pub fields: Vec<Field>,
}

/// Description of a single field within a derived module.
#[derive(Debug, Clone)]
pub struct Field {
    pub ident: Ident,
    pub key: String,
    pub kind: Option<FieldType>,
    pub default: Option<TokenStream>,
    pub init: Option<TokenStream>,
    pub ignore: bool,
    pub span: Span,
}

/// Fully classified field type, including container and scalar information.
#[derive(Debug, Clone)]
pub struct FieldType {
    pub container: ContainerKind,
    pub scalar: ScalarKind,
}

/// High-level container category for a field (plain, option, vector, etc.).
#[derive(Debug, Clone, Copy)]
pub enum ContainerKind {
    Plain,
    Vec,
    Option,
    OptionVec,
}

/// Primitive scalar type available for derived configuration fields.
#[derive(Debug, Clone)]
pub enum ScalarKind {
    String,
    Bool,
    Integer(IntegerKind),
    Float(FloatKind),
    Datetime,
}

/// Supported integer widths mapped from TOML values.
#[derive(Debug, Clone, Copy)]
pub enum IntegerKind {
    I8,
    I16,
    I32,
    I64,
    Isize,
    U8,
    U16,
    U32,
    U64,
    Usize,
}

/// Supported floating-point widths mapped from TOML values.
#[derive(Debug, Clone, Copy)]
pub enum FloatKind {
    F32,
    F64,
}

impl IntegerKind {
    /// Returns the Rust type tokens for the integer variant.
    pub fn type_tokens(&self) -> TokenStream {
        match self {
            Self::I8 => quote::quote!(i8),
            Self::I16 => quote::quote!(i16),
            Self::I32 => quote::quote!(i32),
            Self::I64 => quote::quote!(i64),
            Self::Isize => quote::quote!(isize),
            Self::U8 => quote::quote!(u8),
            Self::U16 => quote::quote!(u16),
            Self::U32 => quote::quote!(u32),
            Self::U64 => quote::quote!(u64),
            Self::Usize => quote::quote!(usize),
        }
    }
}

impl FloatKind {
    /// Returns the Rust type tokens for the floating-point variant.
    pub fn type_tokens(&self) -> TokenStream {
        match self {
            Self::F32 => quote::quote!(f32),
            Self::F64 => quote::quote!(f64),
        }
    }
}
