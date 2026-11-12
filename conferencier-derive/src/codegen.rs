use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Ident, LitStr, Result};

use crate::model::{
    ContainerKind, Field, FieldKind, FieldType, FloatKind, IntegerKind, ScalarKind, Scope,
    TableContainer, TableType,
};

/// Produces the async load/save implementation for a parsed scope description.
///
/// Loading converts a snapshot of the whole section and saving replaces the whole section, so
/// both operations are atomic with respect to the store (RFC-0004 R1–R5).
pub fn generate(scope: Scope, crate_path: syn::Path) -> Result<TokenStream> {
    let Scope {
        ident,
        generics,
        section,
        fields,
    } = scope;

    let section_lit = LitStr::new(&section, Span::call_site());
    let path = quote! { #section_lit };
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let store = generated_local("store");
    let table = generated_local("table");
    let guard = generated_local("guard");

    let loaded: Vec<_> = fields.iter().filter(|field| !field.ignore).collect();
    let loaded_idents: Vec<_> = loaded.iter().map(|field| &field.ident).collect();
    let loaded_locals: Vec<_> = loaded_idents
        .iter()
        .map(|ident| field_local(ident))
        .collect();
    let load_statements = load_statements(&fields, &path, &table, &crate_path)?;
    let store_statements =
        store_statements(&fields, &quote! { #guard }, &path, &table, &crate_path)?;
    let struct_fields = struct_fields(&fields);

    let crate_private = quote! { #crate_path::__private };
    let shared_store = quote! { #crate_path::ConfigStore };
    let shared_scope = quote! { #crate_path::SharedScope<Self> };
    let result_type = quote! { #crate_path::Result };
    let table_type = quote! { #crate_path::toml::Table };

    let read_section = quote! { #crate_private::read_section(&#store, #section_lit).await? };
    // Without loaded fields the section is only checked (R5); a binding would be unused.
    let snapshot = if loaded.is_empty() {
        quote! { #read_section; }
    } else {
        quote! { let mut #table = #read_section; }
    };

    let assign = if loaded.is_empty() {
        TokenStream::new()
    } else {
        quote! {
            let mut #guard = scope.write().await;
            #( #guard.#loaded_idents = #loaded_locals; )*
        }
    };

    let section_table = if loaded.is_empty() {
        quote! { #table_type::new() }
    } else {
        quote! {
            {
                let #guard = scope.read().await;
                let mut #table = #table_type::new();
                #( #store_statements )*
                #table
            }
        }
    };

    Ok(quote! {
        #[#crate_private::async_trait]
        impl #impl_generics #crate_path::ConfigScope for #ident #ty_generics #where_clause {
            async fn from_store(#store: #shared_store) -> #result_type<#shared_scope> {
                #snapshot
                #( #load_statements )*
                let value = Self { #(#struct_fields),* };
                Ok(#crate_private::new_shared_scope(value))
            }

            async fn load(scope: &#shared_scope, #store: #shared_store) -> #result_type<()> {
                #snapshot
                #( #load_statements )*
                #assign
                Ok(())
            }

            async fn save(scope: &#shared_scope, #store: #shared_store) -> #result_type<()> {
                let #table = #section_table;
                #store.set_section_table(#section_lit, #table).await
            }
        }
    })
}

/// Produces the synchronous `ConfigTable` implementation for a parsed struct description.
///
/// It uses the same field conversions as `ConfigScope`, with the `path` argument in place of
/// the section name.
pub fn generate_table(scope: Scope, crate_path: syn::Path) -> Result<TokenStream> {
    let Scope {
        ident,
        generics,
        fields,
        ..
    } = scope;

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let path = generated_local("path");
    let table = generated_local("table");
    let path_expr = quote! { #path };
    let load_statements = load_statements(&fields, &path_expr, &table, &crate_path)?;
    let store_statements =
        store_statements(&fields, &quote! { self }, &path_expr, &table, &crate_path)?;
    let struct_fields = struct_fields(&fields);
    let table_type = quote! { #crate_path::toml::Table };
    let result_type = quote! { #crate_path::Result };

    // Unused parameters are left unnamed, so no lint fires in the user's crate.
    let has_loaded = fields.iter().any(|field| !field.ignore);
    let from_params = if has_loaded {
        quote! { #path: &str, mut #table: #table_type }
    } else {
        quote! { _: &str, _: #table_type }
    };
    let to_path = if fields
        .iter()
        .filter_map(|field| field.kind.as_ref())
        .any(store_uses_path)
    {
        quote! { #path }
    } else {
        quote! { _ }
    };
    let to_body = if has_loaded {
        quote! {
            let mut #table = #table_type::new();
            #( #store_statements )*
            Ok(#table)
        }
    } else {
        quote! { Ok(#table_type::new()) }
    };

    Ok(quote! {
        impl #impl_generics #crate_path::ConfigTable for #ident #ty_generics #where_clause {
            fn from_table(#from_params) -> #result_type<Self> {
                #( #load_statements )*
                Ok(Self { #(#struct_fields),* })
            }

            fn to_table(&self, #to_path: &str) -> #result_type<#table_type> {
                #to_body
            }
        }
    })
}

/// Field initializers of the generated `Self { ... }` expression.
///
/// Loaded fields take their converted locals; ignored fields are initialized afterwards.
fn struct_fields(fields: &[Field]) -> Vec<TokenStream> {
    fields
        .iter()
        .map(|field| {
            let ident = &field.ident;
            if field.ignore {
                let value = ignored_value(field);
                quote! { #ident: #value }
            } else {
                let local = field_local(ident);
                quote! { #ident: #local }
            }
        })
        .collect()
}

/// Returns `true` when saving a field of this kind can report an error with its path.
fn store_uses_path(kind: &FieldKind) -> bool {
    match kind {
        FieldKind::Scalar(kind) => matches!(
            kind.scalar,
            ScalarKind::Integer(IntegerKind::U64 | IntegerKind::Usize)
        ),
        FieldKind::Table(_) => true,
    }
}

/// Name of the local holding a field's value inside the generated functions.
///
/// The prefix keeps user field names (e.g. `store`, `scope`) from shadowing the identifiers
/// used by the generated code. The mixed-site span additionally hides the local from user
/// expressions, which carry call-site spans.
fn field_local(ident: &Ident) -> Ident {
    format_ident!("__confer_{}", ident, span = Span::mixed_site())
}

/// Identifier of a local or parameter of the generated code, such as `store` or `table`.
///
/// It has a mixed-site span, so initializers from `#[confer(ignore = <expr>)]` cannot refer to it.
fn generated_local(name: &str) -> Ident {
    Ident::new(name, Span::mixed_site())
}

/// Generates the initial value of an ignored field.
///
/// An initializer runs in a closure that returns the field type: type errors then point at the
/// user's expression, and `return` or `?` cannot leave the generated function.
fn ignored_value(field: &Field) -> TokenStream {
    let ty = &field.ty;
    match &field.initializer {
        Some(init) => quote! { (|| -> #ty { #init })() },
        None => quote_spanned! { ty.span() => ::core::default::Default::default() },
    }
}

/// Returns the conversion of a loaded field.
fn field_kind(field: &Field) -> Result<&FieldKind> {
    field
        .kind
        .as_ref()
        .ok_or_else(|| syn::Error::new(field.span, "internal error: missing field kind"))
}

/// Generates statements binding every loaded field to its local, in declaration order.
///
/// Each statement moves the field's value out of the owned `table` and converts it, applying
/// the default, `None`, `MissingKey` or empty-table behavior for absent keys. `path` is a `&str`
/// expression naming `table` in errors. Errors are returned from the enclosing function.
fn load_statements(
    fields: &[Field],
    path: &TokenStream,
    table: &Ident,
    crate_path: &syn::Path,
) -> Result<Vec<TokenStream>> {
    fields
        .iter()
        .filter(|field| !field.ignore)
        .map(|field| {
            let local = field_local(&field.ident);
            let key = LitStr::new(&field.key, field.span);
            let (present, absent) = match field_kind(field)? {
                FieldKind::Scalar(kind) => (
                    present_value(kind, path, &key, crate_path),
                    absent_value(kind, field.default.as_ref(), path, &key, crate_path),
                ),
                FieldKind::Table(table_type) => {
                    table_from_value(table_type, path, &key, crate_path)
                }
            };
            Ok(quote! {
                let #local = match #table.remove(#key) {
                    ::core::option::Option::Some(value) => #present,
                    ::core::option::Option::None => #absent,
                };
            })
        })
        .collect()
}

/// Generates statements inserting every loaded field of `source` into `table`.
///
/// `source` is an expression whose fields are read by reference. `None` options and empty
/// vectors of tables are omitted.
fn store_statements(
    fields: &[Field],
    source: &TokenStream,
    path: &TokenStream,
    table: &Ident,
    crate_path: &syn::Path,
) -> Result<Vec<TokenStream>> {
    fields
        .iter()
        .filter(|field| !field.ignore)
        .map(|field| {
            let ident = &field.ident;
            let key = LitStr::new(&field.key, field.span);
            let block = match field_kind(field)? {
                FieldKind::Scalar(kind) => scalar_insert(kind, table, path, &key, crate_path),
                FieldKind::Table(table_type) => {
                    table_insert(table_type, table, path, &key, crate_path)
                }
            };
            Ok(quote! {
                {
                    let value = &#source.#ident;
                    #block
                }
            })
        })
        .collect()
}

/// Inserts the scalar field referenced by `value` into `table`.
fn scalar_insert(
    kind: &FieldType,
    table: &Ident,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let insert = |value: TokenStream| {
        quote! { #table.insert(::std::string::String::from(#key), #value); }
    };
    match kind.container {
        ContainerKind::Plain => insert(scalar_to_value(&kind.scalar, path, key, crate_path)),
        ContainerKind::Vec => insert(vec_to_value(&kind.scalar, path, key, crate_path)),
        ContainerKind::Option => {
            let insert = insert(scalar_to_value(&kind.scalar, path, key, crate_path));
            quote! {
                if let ::core::option::Option::Some(value) = value {
                    #insert
                }
            }
        }
        ContainerKind::OptionVec => {
            let insert = insert(vec_to_value(&kind.scalar, path, key, crate_path));
            quote! {
                if let ::core::option::Option::Some(value) = value {
                    #insert
                }
            }
        }
    }
}

/// Path to a `ConfigTable` method of the field's inner type.
///
/// The call carries the span of the inner type, so a missing implementation is reported there.
fn table_method(table_type: &TableType, method: &str, crate_path: &syn::Path) -> TokenStream {
    let inner = &table_type.inner;
    let method = Ident::new(method, Span::call_site());
    quote_spanned! { inner.span() =>
        <#inner as #crate_path::ConfigTable>::#method
    }
}

/// Converts a present TOML `value` of a table field and handles its absence.
///
/// Returns the expressions for the present and the absent case. Array elements are checked and
/// converted one at a time, so the first error in declaration order is reported (L5).
fn table_from_value(
    table_type: &TableType,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> (TokenStream, TokenStream) {
    let private = quote! { #crate_path::__private };
    let from_table = table_method(table_type, "from_table", crate_path);
    let nested = quote! {
        {
            let value = #private::table(#path, #key, value)?;
            #from_table(&#private::child_path(#path, #key), value)?
        }
    };
    match table_type.container {
        TableContainer::Plain => (
            nested,
            quote! {
                #from_table(&#private::child_path(#path, #key), #crate_path::toml::Table::new())?
            },
        ),
        TableContainer::Option => (
            quote! { ::core::option::Option::Some(#nested) },
            quote! { ::core::option::Option::None },
        ),
        TableContainer::Vec => (
            quote! {
                {
                    let value = #private::array(#path, #key, value)?;
                    let array_path = #private::child_path(#path, #key);
                    let mut out = ::std::vec::Vec::with_capacity(value.len());
                    for (index, value) in value.into_iter().enumerate() {
                        let value = #private::table_element(#path, #key, value, index)?;
                        out.push(#from_table(&#private::element_path(&array_path, index), value)?);
                    }
                    out
                }
            },
            quote! { ::std::vec::Vec::new() },
        ),
    }
}

/// Inserts the table field referenced by `value` into `table`.
///
/// A plain table is always written; `None` and an empty vector are omitted.
fn table_insert(
    table_type: &TableType,
    table: &Ident,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let private = quote! { #crate_path::__private };
    let value_type = quote! { #crate_path::toml::Value };
    let to_table = table_method(table_type, "to_table", crate_path);
    let insert_nested = quote! {
        #table.insert(
            ::std::string::String::from(#key),
            #value_type::Table(#to_table(value, &#private::child_path(#path, #key))?),
        );
    };
    match table_type.container {
        TableContainer::Plain => insert_nested,
        TableContainer::Option => quote! {
            if let ::core::option::Option::Some(value) = value {
                #insert_nested
            }
        },
        TableContainer::Vec => quote! {
            if !value.is_empty() {
                let array_path = #private::child_path(#path, #key);
                let mut array = ::std::vec::Vec::with_capacity(value.len());
                for (index, value) in value.iter().enumerate() {
                    array.push(#value_type::Table(
                        #to_table(value, &#private::element_path(&array_path, index))?,
                    ));
                }
                #table.insert(::std::string::String::from(#key), #value_type::Array(array));
            }
        },
    }
}

/// Converts a present TOML `value` into the field's Rust type.
fn present_value(
    kind: &FieldType,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let (convert, adjust) = match kind.container {
        ContainerKind::Plain | ContainerKind::Option => (
            scalar_conversion(&kind.scalar),
            scalar_from_store(&kind.scalar, path, key, crate_path),
        ),
        ContainerKind::Vec | ContainerKind::OptionVec => (
            vec_conversion(&kind.scalar),
            vec_from_store(&kind.scalar, path, key, crate_path),
        ),
    };
    let convert = Ident::new(convert, Span::call_site());
    let converted = quote! {
        {
            let value = #crate_path::__private::#convert(#path, #key, value)?;
            #adjust
        }
    };
    match kind.container {
        ContainerKind::Plain | ContainerKind::Vec => converted,
        ContainerKind::Option | ContainerKind::OptionVec => {
            quote! { ::core::option::Option::Some(#converted) }
        }
    }
}

/// Handles missing keys by applying defaults, converting to `None` or returning `MissingKey`.
fn absent_value(
    kind: &FieldType,
    default: Option<&TokenStream>,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    match (kind.container, default) {
        (_, Some(default)) => quote! { #default },
        (ContainerKind::Plain | ContainerKind::Vec, None) => {
            quote! { return Err(#crate_path::Error::missing_key(#path, #key)) }
        }
        (ContainerKind::Option | ContainerKind::OptionVec, None) => {
            quote! { ::core::option::Option::None }
        }
    }
}

/// Name of the `__private` conversion from a TOML value to the scalar's TOML representation.
fn scalar_conversion(scalar: &ScalarKind) -> &'static str {
    match scalar {
        ScalarKind::String => "string",
        ScalarKind::Bool => "boolean",
        ScalarKind::Integer(_) => "integer",
        ScalarKind::Float(_) => "float",
        ScalarKind::Datetime => "datetime",
    }
}

/// Name of the `__private` conversion from a TOML array to a vector of the scalar.
fn vec_conversion(scalar: &ScalarKind) -> &'static str {
    match scalar {
        ScalarKind::String => "string_vec",
        ScalarKind::Bool => "boolean_vec",
        ScalarKind::Integer(_) => "integer_vec",
        ScalarKind::Float(_) => "float_vec",
        ScalarKind::Datetime => "datetime_vec",
    }
}

/// Converts the scalar referenced by `value` into a TOML value.
fn scalar_to_value(
    scalar: &ScalarKind,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let value_type = quote! { #crate_path::toml::Value };
    match scalar {
        ScalarKind::String => {
            quote! { #value_type::String(::core::clone::Clone::clone(value)) }
        }
        ScalarKind::Bool => quote! { #value_type::Boolean(*value) },
        ScalarKind::Datetime => {
            quote! { #value_type::Datetime(::core::clone::Clone::clone(value)) }
        }
        ScalarKind::Integer(kind) => {
            let raw = integer_to_store(kind, quote! { (*value) }, path, key, crate_path);
            quote! { #value_type::Integer(#raw) }
        }
        ScalarKind::Float(FloatKind::F64) => quote! { #value_type::Float(*value) },
        ScalarKind::Float(FloatKind::F32) => quote! { #value_type::Float(f64::from(*value)) },
    }
}

/// Converts the vector referenced by `value` into a TOML array.
fn vec_to_value(
    scalar: &ScalarKind,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let element = scalar_to_value(scalar, path, key, crate_path);
    quote! {
        {
            let mut array = ::std::vec::Vec::with_capacity(value.len());
            for value in value {
                array.push(#element);
            }
            #crate_path::toml::Value::Array(array)
        }
    }
}

/// Applies container-specific conversions for scalar fields.
fn scalar_from_store(
    scalar: &ScalarKind,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    match scalar {
        ScalarKind::String | ScalarKind::Bool | ScalarKind::Datetime => quote! { value },
        ScalarKind::Integer(kind) => integer_from_store(kind, path, key, crate_path),
        ScalarKind::Float(kind) => float_from_store(kind, path, key, crate_path),
    }
}

/// Applies container-specific conversions for vector fields.
fn vec_from_store(
    scalar: &ScalarKind,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    match scalar {
        ScalarKind::String | ScalarKind::Bool | ScalarKind::Datetime => quote! { value },
        ScalarKind::Integer(kind) => integer_vec_from_store(kind, path, key, crate_path),
        ScalarKind::Float(kind) => float_vec_from_store(kind, path, key, crate_path),
    }
}

/// Validates and converts TOML integers into the appropriate Rust integer type.
fn integer_from_store(
    kind: &IntegerKind,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let err = quote! { #crate_path::Error };
    let ty = kind.type_tokens();
    quote! {
        match <#ty as ::core::convert::TryFrom<i64>>::try_from(value) {
            Ok(v) => v,
            Err(_) => {
                return Err(#err::value_parse(#path, #key, format!("value out of range for {}", stringify!(#ty))));
            }
        }
    }
}

/// Validates and converts TOML floats into the requested Rust float type.
fn float_from_store(
    kind: &FloatKind,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    match kind {
        FloatKind::F64 => quote! { value },
        FloatKind::F32 => {
            let err = quote! { #crate_path::Error };
            quote! {
                {
                    let raw = value;
                    if !raw.is_finite() {
                        return Err(#err::value_parse(#path, #key, String::from("non-finite float")));
                    }
                    if raw < f32::MIN as f64 || raw > f32::MAX as f64 {
                        return Err(#err::value_parse(#path, #key, String::from("value out of range for f32")));
                    }
                    raw as f32
                }
            }
        }
    }
}

/// Validates and converts TOML integer arrays into typed Rust vectors.
fn integer_vec_from_store(
    kind: &IntegerKind,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let err = quote! { #crate_path::Error };
    let scalar = kind.type_tokens();
    quote! {
        {
            let mut out = Vec::with_capacity(value.len());
            for raw in value.into_iter() {
                match <#scalar as ::core::convert::TryFrom<i64>>::try_from(raw) {
                    Ok(v) => out.push(v),
                    Err(_) => {
                        return Err(#err::value_parse(#path, #key, format!("value out of range for {}", stringify!(#scalar))));
                    }
                }
            }
            out
        }
    }
}

/// Validates and converts TOML float arrays into typed Rust vectors.
fn float_vec_from_store(
    kind: &FloatKind,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    match kind {
        FloatKind::F64 => quote! { value },
        FloatKind::F32 => {
            let err = quote! { #crate_path::Error };
            quote! {
                {
                    let mut out = Vec::with_capacity(value.len());
                    for raw in value.into_iter() {
                        if !raw.is_finite() {
                            return Err(#err::value_parse(#path, #key, String::from("non-finite float")));
                        }
                        if raw < f32::MIN as f64 || raw > f32::MAX as f64 {
                            return Err(#err::value_parse(#path, #key, String::from("value out of range for f32")));
                        }
                        out.push(raw as f32);
                    }
                    out
                }
            }
        }
    }
}

/// Widens a Rust integer to TOML's signed 64-bit representation, checking ranges when needed.
fn integer_to_store(
    kind: &IntegerKind,
    value: TokenStream,
    path: &TokenStream,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let err = quote! { #crate_path::Error };
    match kind {
        IntegerKind::I64 | IntegerKind::Isize => quote! { #value as i64 },
        IntegerKind::I8 | IntegerKind::I16 | IntegerKind::I32 => quote! { i64::from(#value) },
        IntegerKind::U8 | IntegerKind::U16 | IntegerKind::U32 => quote! { #value as i64 },
        IntegerKind::U64 | IntegerKind::Usize => {
            quote! {
                {
                    let raw = #value as u64;
                    if raw > i64::MAX as u64 {
                        return Err(#err::value_parse(#path, #key, format!("value `{}` out of range for TOML integer", raw)));
                    }
                    raw as i64
                }
            }
        }
    }
}
