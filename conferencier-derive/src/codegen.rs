use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Ident, LitStr, Result};

use crate::model::{ContainerKind, Field, FieldType, FloatKind, IntegerKind, Module, ScalarKind};

/// Produces the async load/save implementation for a parsed module description.
///
/// Loading converts a snapshot of the whole section and saving replaces the whole section, so
/// both operations are atomic with respect to the store (RFC-0004 R1–R5).
pub fn generate(module: Module, crate_path: syn::Path) -> Result<TokenStream> {
    let Module {
        ident,
        generics,
        section,
        fields,
    } = module;

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

    let struct_fields = fields.iter().map(|field| {
        let ident = &field.ident;
        if field.ignore {
            let value = ignored_value(field);
            quote! { #ident: #value }
        } else {
            let local = field_local(ident);
            quote! { #ident: #local }
        }
    });

    let crate_private = quote! { #crate_path::__private };
    let shared_confer = quote! { #crate_path::SharedConfer };
    let shared_module = quote! { #crate_path::confer_module::SharedConferModule<Self> };
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
            let mut #guard = module.write().await;
            #( #guard.#loaded_idents = #loaded_locals; )*
        }
    };

    let section_table = if loaded.is_empty() {
        quote! { #table_type::new() }
    } else {
        quote! {
            {
                let #guard = module.read().await;
                let mut #table = #table_type::new();
                #( #store_statements )*
                #table
            }
        }
    };

    Ok(quote! {
        #[#crate_private::async_trait]
        impl #impl_generics #crate_path::confer_module::ConferModule for #ident #ty_generics #where_clause {
            async fn from_confer(#store: #shared_confer) -> #result_type<#shared_module> {
                #snapshot
                #( #load_statements )*
                let value = Self { #(#struct_fields),* };
                Ok(#crate_private::new_shared_module(value))
            }

            async fn load(module: &#shared_module, #store: #shared_confer) -> #result_type<()> {
                #snapshot
                #( #load_statements )*
                #assign
                Ok(())
            }

            async fn save(module: &#shared_module, #store: #shared_confer) -> #result_type<()> {
                let #table = #section_table;
                #store.set_section_table(#section_lit, #table).await
            }
        }
    })
}

/// Name of the local holding a field's value inside the generated functions.
///
/// The prefix keeps user field names (e.g. `store`, `module`) from shadowing the identifiers
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

/// Returns the classified type of a loaded field.
fn field_kind(field: &Field) -> Result<&FieldType> {
    field
        .kind
        .as_ref()
        .ok_or_else(|| syn::Error::new(field.span, "internal error: missing field kind"))
}

/// Generates statements binding every loaded field to its local, in declaration order.
///
/// Each statement moves the field's value out of the owned `table` and converts it, applying
/// the default, `None` or `MissingKey` behavior for absent keys. `path` is a `&str` expression
/// naming `table` in errors. Errors are returned from the enclosing function.
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
            let kind = field_kind(field)?;
            let local = field_local(&field.ident);
            let key = LitStr::new(&field.key, field.span);
            let present = present_value(kind, path, &key, crate_path);
            let absent = absent_value(kind, field.default.as_ref(), path, &key, crate_path);
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
/// `source` is an expression whose fields are read by reference. `None` options are omitted.
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
            let kind = field_kind(field)?;
            let ident = &field.ident;
            let key = LitStr::new(&field.key, field.span);
            let insert = |value: TokenStream| {
                quote! { #table.insert(::std::string::String::from(#key), #value); }
            };
            let block = match kind.container {
                ContainerKind::Plain => {
                    insert(scalar_to_value(&kind.scalar, path, &key, crate_path))
                }
                ContainerKind::Vec => insert(vec_to_value(&kind.scalar, path, &key, crate_path)),
                ContainerKind::Option => {
                    let insert = insert(scalar_to_value(&kind.scalar, path, &key, crate_path));
                    quote! {
                        if let ::core::option::Option::Some(value) = value {
                            #insert
                        }
                    }
                }
                ContainerKind::OptionVec => {
                    let insert = insert(vec_to_value(&kind.scalar, path, &key, crate_path));
                    quote! {
                        if let ::core::option::Option::Some(value) = value {
                            #insert
                        }
                    }
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
            quote! { return Err(#crate_path::ConferError::missing_key(#path, #key)) }
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
    let err = quote! { #crate_path::ConferError };
    let ty = kind.type_tokens();
    quote! {
        match <#ty as ::core::convert::TryFrom<i64>>::try_from(value) {
            Ok(v) => v,
            Err(_) => {
                return Err(#err::value_parse_owned(#path, #key, format!("value out of range for {}", stringify!(#ty))));
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
            let err = quote! { #crate_path::ConferError };
            quote! {
                {
                    let raw = value;
                    if !raw.is_finite() {
                        return Err(#err::value_parse_owned(#path, #key, String::from("non-finite float")));
                    }
                    if raw < f32::MIN as f64 || raw > f32::MAX as f64 {
                        return Err(#err::value_parse_owned(#path, #key, String::from("value out of range for f32")));
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
    let err = quote! { #crate_path::ConferError };
    let scalar = kind.type_tokens();
    quote! {
        {
            let mut out = Vec::with_capacity(value.len());
            for raw in value.into_iter() {
                match <#scalar as ::core::convert::TryFrom<i64>>::try_from(raw) {
                    Ok(v) => out.push(v),
                    Err(_) => {
                        return Err(#err::value_parse_owned(#path, #key, format!("value out of range for {}", stringify!(#scalar))));
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
            let err = quote! { #crate_path::ConferError };
            quote! {
                {
                    let mut out = Vec::with_capacity(value.len());
                    for raw in value.into_iter() {
                        if !raw.is_finite() {
                            return Err(#err::value_parse_owned(#path, #key, String::from("non-finite float")));
                        }
                        if raw < f32::MIN as f64 || raw > f32::MAX as f64 {
                            return Err(#err::value_parse_owned(#path, #key, String::from("value out of range for f32")));
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
    let err = quote! { #crate_path::ConferError };
    match kind {
        IntegerKind::I64 | IntegerKind::Isize => quote! { #value as i64 },
        IntegerKind::I8 | IntegerKind::I16 | IntegerKind::I32 => quote! { i64::from(#value) },
        IntegerKind::U8 | IntegerKind::U16 | IntegerKind::U32 => quote! { #value as i64 },
        IntegerKind::U64 | IntegerKind::Usize => {
            quote! {
                {
                    let raw = #value as u64;
                    if raw > i64::MAX as u64 {
                        return Err(#err::value_parse_owned(#path, #key, format!("value `{}` out of range for TOML integer", raw)));
                    }
                    raw as i64
                }
            }
        }
    }
}
