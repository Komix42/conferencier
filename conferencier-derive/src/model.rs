use proc_macro2::{Span, TokenStream};
use syn::{Ident, Type};

/// Derive macro being expanded; selects error messages and the generated trait.
#[derive(Debug, Clone, Copy)]
pub enum DeriveKind {
    Scope,
    Table,
}

impl DeriveKind {
    /// Name of the derive macro, used in error messages.
    pub fn name(self) -> &'static str {
        match self {
            Self::Scope => "ConfigScope",
            Self::Table => "ConfigTable",
        }
    }
}

/// Intermediate representation of a struct annotated with `#[derive(ConfigScope)]` or
/// `#[derive(ConfigTable)]`.
#[derive(Debug, Clone)]
pub struct Scope {
    pub ident: Ident,
    pub generics: syn::Generics,
    /// TOML section of a scope; validated but unused for tables.
    pub section: String,
    /// Rejects keys not mapped to loaded fields during table conversion.
    pub deny_unknown_keys: bool,
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
    /// Explicit fallback for a missing key; only present on loaded fields.
    pub default: Option<FieldDefault>,
    /// Loads a missing single table from an empty table.
    pub field_defaults: bool,
    /// Expression from `#[confer(ignore = <expr>)]`; only present on ignored fields.
    pub initializer: Option<TokenStream>,
    pub ignore: bool,
    pub span: Span,
}

/// A bare `default` or an expression, with the attribute span for diagnostics.
#[derive(Debug, Clone)]
pub struct FieldDefault {
    pub expr: Option<TokenStream>,
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

/// Type of a `#[confer(table)]` field: a container around a type implementing `ConfigTable`.
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
}
