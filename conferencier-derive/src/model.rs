use proc_macro2::{Span, TokenStream};
use syn::{Ident, Type};

/// Derive macro being expanded; selects error messages and the generated trait.
#[derive(Debug, Clone, Copy)]
pub enum DeriveKind {
    Module,
    Table,
}

impl DeriveKind {
    /// Name of the derive macro, used in error messages.
    pub fn name(self) -> &'static str {
        match self {
            Self::Module => "ConferModule",
            Self::Table => "ConferTable",
        }
    }
}

/// Intermediate representation of a struct annotated with `#[derive(ConferModule)]` or
/// `#[derive(ConferTable)]`.
#[derive(Debug, Clone)]
pub struct Module {
    pub ident: Ident,
    pub generics: syn::Generics,
    /// TOML section of a module; validated but unused for tables.
    pub section: String,
    pub fields: Vec<Field>,
}

/// Description of a single field within a derived struct.
#[derive(Debug, Clone)]
pub struct Field {
    pub ident: Ident,
    pub ty: Type,
    pub key: String,
    /// Conversion of a loaded field; `None` for ignored fields.
    pub kind: Option<FieldKind>,
    /// Literal fallback for a missing key; only present on loaded scalar fields.
    pub default: Option<TokenStream>,
    /// Expression from `#[confer(ignore = <expr>)]`; only present on ignored fields.
    pub initializer: Option<TokenStream>,
    pub ignore: bool,
    pub span: Span,
}

/// How a loaded field is converted from and to TOML.
#[derive(Debug, Clone)]
pub enum FieldKind {
    Scalar(FieldType),
    /// Field marked `#[confer(table)]`.
    Table(Box<TableType>),
}

/// Fully classified field type, including container and scalar information.
#[derive(Debug, Clone)]
pub struct FieldType {
    pub container: ContainerKind,
    pub scalar: ScalarKind,
}

/// Type of a `#[confer(table)]` field: a container around a type implementing `ConferTable`.
#[derive(Debug, Clone)]
pub struct TableType {
    pub container: TableContainer,
    pub inner: Type,
}

/// Container shapes supported for table fields.
#[derive(Debug, Clone, Copy)]
pub enum TableContainer {
    Plain,
    Option,
    Vec,
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

    /// Returns `true` for unsigned integer types.
    pub fn is_unsigned(&self) -> bool {
        matches!(
            self,
            Self::U8 | Self::U16 | Self::U32 | Self::U64 | Self::Usize
        )
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
