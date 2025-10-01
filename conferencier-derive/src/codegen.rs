use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, LitStr, Result};

use crate::model::{ContainerKind, Field, FieldType, FloatKind, IntegerKind, Module, ScalarKind};

/// Produces the async load/save implementation for a parsed module description.
pub fn generate(module: Module, crate_path: syn::Path) -> Result<TokenStream> {
    let Module {
        ident,
        generics,
        section,
        fields,
    } = module;

    let section_lit = LitStr::new(&section, Span::call_site());
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let init_fields = fields.iter().map(|field| {
        let ident = &field.ident;
        if let Some(init) = &field.init {
            quote! { #ident: #init }
        } else if let Some(default) = &field.default {
            quote! { #ident: #default }
        } else {
            quote! { #ident: ::core::default::Default::default() }
        }
    });

    let load_blocks: Vec<_> = fields
        .iter()
        .filter(|field| !field.ignore)
        .map(|field| generate_load(field, &section_lit, &crate_path))
        .collect::<Result<_>>()?;

    let save_blocks: Vec<_> = fields
        .iter()
        .filter(|field| !field.ignore)
        .map(|field| generate_save(field, &section_lit, &crate_path))
        .collect::<Result<_>>()?;

    let owned_keys: Vec<_> = fields
        .iter()
        .filter(|field| !field.ignore)
        .map(|field| LitStr::new(&field.key, field.span))
        .collect();

    let known_keys_expr = if owned_keys.is_empty() {
        quote! { &[] as &[&str] }
    } else {
        quote! { &[#(#owned_keys),*] }
    };

    let clone_block = generate_clone_block(&fields);

    let crate_private = quote! { #crate_path::__private };
    let shared_confer = quote! { #crate_path::SharedConfer };
    let shared_module = quote! { #crate_path::confer_module::SharedConferModule<Self> };
    let result_type = quote! { #crate_path::Result };

    Ok(quote! {
        #[#crate_private::async_trait]
        impl #impl_generics #crate_path::confer_module::ConferModule for #ident #ty_generics #where_clause {
            async fn from_confer(store: #shared_confer) -> #result_type<#shared_module> {
                let value = Self { #(#init_fields),* };
                let module = #crate_private::new_shared_module(value);
                Self::load(&module, store).await?;
                Ok(module)
            }

            async fn load(module: &#shared_module, store: #shared_confer) -> #result_type<()> {
                #( #load_blocks )*
                Ok(())
            }

            async fn save(module: &#shared_module, store: #shared_confer) -> #result_type<()> {
                store.add_section(#section_lit).await?;
                #clone_block
                #( #save_blocks )*

                let existing = store.list_keys(#section_lit).await?;
                for key in existing {
                    if !(#known_keys_expr).contains(&key.as_str()) {
                        store.remove_key(#section_lit, &key).await?;
                    }
                }
                Ok(())
            }
        }
    })
}

/// Produces the `let (...) = { ... }` block cloning mutable fields for persistence.
fn generate_clone_block(fields: &[Field]) -> TokenStream {
    let locals: Vec<_> = fields
        .iter()
        .filter(|field| !field.ignore)
        .map(|field| field.ident.clone())
        .collect();

    if locals.is_empty() {
        return quote! {
            let guard = module.read().await;
            drop(guard);
        };
    }

    quote! {
        let (#(#locals),*) = {
            let guard = module.read().await;
            ( #(guard.#locals.clone()),* )
        };
    }
}

/// Generates the load logic for a single field, including defaults and conversions.
fn generate_load(field: &Field, section: &LitStr, crate_path: &syn::Path) -> Result<TokenStream> {
    let Field {
        ident,
        key,
        kind,
        default,
        ..
    } = field;

    let kind = kind
        .as_ref()
        .ok_or_else(|| syn::Error::new(field.span, "internal error: missing field kind"))?;

    let key_lit = LitStr::new(key, field.span);
    let fetch = fetch_expression(kind, section, &key_lit);
    let converted = convert_from_store(kind, section, &key_lit, crate_path);
    let assign = assign_converted(kind, ident);
    let on_missing = missing_behavior(kind, ident, default.as_ref(), section, &key_lit, crate_path);

    Ok(quote! {
        match #fetch {
            Ok(value) => {
                let converted = { #converted };
                let mut guard = module.write().await;
                #assign
            }
            Err(err) => match err {
                #crate_path::ConferError::MissingKey { .. } => { #on_missing }
                other => return Err(other),
            },
        }
    })
}

/// Generates the save logic for a single field, respecting optionality and vectors.
fn generate_save(field: &Field, section: &LitStr, crate_path: &syn::Path) -> Result<TokenStream> {
    let Field {
        ident, key, kind, ..
    } = field;

    let kind = kind
        .as_ref()
        .ok_or_else(|| syn::Error::new(field.span, "internal error: missing field kind"))?;

    let key_lit = LitStr::new(key, field.span);

    let block = match kind.container {
        ContainerKind::Plain => save_plain(kind, ident, section, &key_lit, crate_path),
        ContainerKind::Vec => save_vec(kind, ident, section, &key_lit, crate_path),
        ContainerKind::Option => save_option(kind, ident, section, &key_lit, crate_path),
        ContainerKind::OptionVec => save_option_vec(kind, ident, section, &key_lit, crate_path),
    };

    Ok(block)
}

/// Selects the appropriate async getter call for a field based on its kind.
fn fetch_expression(kind: &FieldType, section: &LitStr, key: &LitStr) -> TokenStream {
    let method = match (kind.container, &kind.scalar) {
        (ContainerKind::Vec, ScalarKind::String)
        | (ContainerKind::OptionVec, ScalarKind::String) => "get_string_vec",
        (ContainerKind::Vec, ScalarKind::Bool) | (ContainerKind::OptionVec, ScalarKind::Bool) => {
            "get_boolean_vec"
        }
        (ContainerKind::Vec, ScalarKind::Integer(_))
        | (ContainerKind::OptionVec, ScalarKind::Integer(_)) => "get_integer_vec",
        (ContainerKind::Vec, ScalarKind::Float(_))
        | (ContainerKind::OptionVec, ScalarKind::Float(_)) => "get_float_vec",
        (ContainerKind::Vec, ScalarKind::Datetime)
        | (ContainerKind::OptionVec, ScalarKind::Datetime) => "get_datetime_vec",
        (_, ScalarKind::String) => "get_string",
        (_, ScalarKind::Bool) => "get_boolean",
        (_, ScalarKind::Integer(_)) => "get_integer",
        (_, ScalarKind::Float(_)) => "get_float",
        (_, ScalarKind::Datetime) => "get_datetime",
    };

    let ident = Ident::new(method, Span::call_site());
    quote! { store.#ident(#section, #key).await }
}

/// Converts the raw value obtained from the store into the field's Rust type.
fn convert_from_store(
    kind: &FieldType,
    section: &LitStr,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    match kind.container {
        ContainerKind::Plain | ContainerKind::Option => {
            scalar_from_store(&kind.scalar, section, key, crate_path)
        }
        ContainerKind::Vec | ContainerKind::OptionVec => {
            vec_from_store(&kind.scalar, section, key, crate_path)
        }
    }
}

/// Emits the assignment into the guard, taking optional containers into account.
fn assign_converted(kind: &FieldType, ident: &Ident) -> TokenStream {
    match kind.container {
        ContainerKind::Plain | ContainerKind::Vec => quote! { guard.#ident = converted; },
        ContainerKind::Option | ContainerKind::OptionVec => {
            quote! { guard.#ident = ::core::option::Option::Some(converted); }
        }
    }
}

/// Handles missing keys by applying defaults or converting to `None`.
fn missing_behavior(
    kind: &FieldType,
    ident: &Ident,
    default: Option<&TokenStream>,
    section: &LitStr,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    match kind.container {
        ContainerKind::Plain | ContainerKind::Vec => {
            if let Some(default) = default {
                quote! {
                    let mut guard = module.write().await;
                    guard.#ident = #default;
                }
            } else {
                quote! { return Err(#crate_path::ConferError::missing_key(#section, #key)); }
            }
        }
        ContainerKind::Option | ContainerKind::OptionVec => {
            if let Some(default) = default {
                quote! {
                    let mut guard = module.write().await;
                    guard.#ident = #default;
                }
            } else {
                quote! {
                    let mut guard = module.write().await;
                    guard.#ident = ::core::option::Option::None;
                }
            }
        }
    }
}

/// Saves scalar fields back into the store.
fn save_plain(
    kind: &FieldType,
    ident: &Ident,
    section: &LitStr,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let setter = setter_name(kind, false);
    let setter_ident = Ident::new(setter, Span::call_site());
    let value = scalar_to_store(&kind.scalar, quote! { #ident }, section, key, crate_path);
    quote! {
        store.#setter_ident(#section, #key, #value).await?;
    }
}

/// Persists vector fields into the store.
fn save_vec(
    kind: &FieldType,
    ident: &Ident,
    section: &LitStr,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let setter = setter_name(kind, true);
    let setter_ident = Ident::new(setter, Span::call_site());
    let value = vec_to_store(&kind.scalar, quote! { #ident }, section, key, crate_path);
    quote! {
        store.#setter_ident(#section, #key, #value).await?;
    }
}

/// Persists `Option<T>` fields, removing keys when the value is `None`.
fn save_option(
    kind: &FieldType,
    ident: &Ident,
    section: &LitStr,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let setter = setter_name(kind, false);
    let setter_ident = Ident::new(setter, Span::call_site());
    let value = scalar_to_store(
        &kind.scalar,
        quote! { value.clone() },
        section,
        key,
        crate_path,
    );
    quote! {
        match #ident {
            ::core::option::Option::Some(value) => {
                store.#setter_ident(#section, #key, #value).await?;
            }
            ::core::option::Option::None => {
                store.remove_key(#section, #key).await?;
            }
        }
    }
}

/// Persists `Option<Vec<T>>` fields with appropriate key cleanup.
fn save_option_vec(
    kind: &FieldType,
    ident: &Ident,
    section: &LitStr,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let setter = setter_name(kind, true);
    let setter_ident = Ident::new(setter, Span::call_site());
    let value = vec_to_store(
        &kind.scalar,
        quote! { value.clone() },
        section,
        key,
        crate_path,
    );
    quote! {
        match #ident {
            ::core::option::Option::Some(value) => {
                store.#setter_ident(#section, #key, #value).await?;
            }
            ::core::option::Option::None => {
                store.remove_key(#section, #key).await?;
            }
        }
    }
}

/// Resolves the setter method name for a given field.
fn setter_name(kind: &FieldType, vec: bool) -> &'static str {
    match (vec, &kind.scalar) {
        (false, ScalarKind::String) => "set_string",
        (false, ScalarKind::Bool) => "set_boolean",
        (false, ScalarKind::Integer(_)) => "set_integer",
        (false, ScalarKind::Float(_)) => "set_float",
        (false, ScalarKind::Datetime) => "set_datetime",
        (true, ScalarKind::String) => "set_string_vec",
        (true, ScalarKind::Bool) => "set_boolean_vec",
        (true, ScalarKind::Integer(_)) => "set_integer_vec",
        (true, ScalarKind::Float(_)) => "set_float_vec",
        (true, ScalarKind::Datetime) => "set_datetime_vec",
    }
}

/// Applies container-specific conversions for scalar fields.
fn scalar_from_store(
    scalar: &ScalarKind,
    section: &LitStr,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    match scalar {
        ScalarKind::String | ScalarKind::Bool | ScalarKind::Datetime => quote! { value },
        ScalarKind::Integer(kind) => integer_from_store(kind, section, key, crate_path),
        ScalarKind::Float(kind) => float_from_store(kind, section, key, crate_path),
    }
}

/// Applies container-specific conversions for vector fields.
fn vec_from_store(
    scalar: &ScalarKind,
    section: &LitStr,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    match scalar {
        ScalarKind::String | ScalarKind::Bool | ScalarKind::Datetime => quote! { value },
        ScalarKind::Integer(kind) => integer_vec_from_store(kind, section, key, crate_path),
        ScalarKind::Float(kind) => float_vec_from_store(kind, section, key, crate_path),
    }
}

/// Converts Rust scalar values into TOML-friendly types.
fn scalar_to_store(
    scalar: &ScalarKind,
    value: TokenStream,
    section: &LitStr,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    match scalar {
        ScalarKind::String | ScalarKind::Bool | ScalarKind::Datetime => value,
        ScalarKind::Integer(kind) => integer_to_store(kind, value, section, key, crate_path),
        ScalarKind::Float(kind) => float_to_store(kind, value),
    }
}

/// Converts Rust vector values into TOML-friendly arrays.
fn vec_to_store(
    scalar: &ScalarKind,
    value: TokenStream,
    section: &LitStr,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    match scalar {
        ScalarKind::String | ScalarKind::Bool | ScalarKind::Datetime => value,
        ScalarKind::Integer(kind) => integer_vec_to_store(kind, value, section, key, crate_path),
        ScalarKind::Float(kind) => float_vec_to_store(kind, value),
    }
}

/// Validates and converts TOML integers into the appropriate Rust integer type.
fn integer_from_store(
    kind: &IntegerKind,
    section: &LitStr,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let err = quote! { #crate_path::ConferError };
    let ty = kind.type_tokens();
    quote! {
        match <#ty as ::core::convert::TryFrom<i64>>::try_from(value) {
            Ok(v) => v,
            Err(_) => {
                return Err(#err::value_parse_owned(#section, #key, format!("value out of range for {}", stringify!(#ty))));
            }
        }
    }
}

/// Validates and converts TOML floats into the requested Rust float type.
fn float_from_store(
    kind: &FloatKind,
    section: &LitStr,
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
                        return Err(#err::value_parse_owned(#section, #key, String::from("non-finite float")));
                    }
                    if raw < f32::MIN as f64 || raw > f32::MAX as f64 {
                        return Err(#err::value_parse_owned(#section, #key, String::from("value out of range for f32")));
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
    section: &LitStr,
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
                        return Err(#err::value_parse_owned(#section, #key, format!("value out of range for {}", stringify!(#scalar))));
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
    section: &LitStr,
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
                            return Err(#err::value_parse_owned(#section, #key, String::from("non-finite float")));
                        }
                        if raw < f32::MIN as f64 || raw > f32::MAX as f64 {
                            return Err(#err::value_parse_owned(#section, #key, String::from("value out of range for f32")));
                        }
                        out.push(raw as f32);
                    }
                    out
                }
            }
        }
    }
}

/// Widens Rust integers to TOML's signed 64-bit representation, checking ranges when needed.
fn integer_to_store(
    kind: &IntegerKind,
    value: TokenStream,
    section: &LitStr,
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
                        return Err(#err::value_parse_owned(#section, #key, format!("value `{}` out of range for TOML integer", raw)));
                    }
                    raw as i64
                }
            }
        }
    }
}

/// Widens Rust floats to TOML's `f64` representation.
fn float_to_store(kind: &FloatKind, value: TokenStream) -> TokenStream {
    match kind {
        FloatKind::F64 => value,
        FloatKind::F32 => quote! { f64::from(#value) },
    }
}

/// Converts Rust integer vectors into TOML integer arrays with range validation.
fn integer_vec_to_store(
    kind: &IntegerKind,
    value: TokenStream,
    section: &LitStr,
    key: &LitStr,
    crate_path: &syn::Path,
) -> TokenStream {
    let err = quote! { #crate_path::ConferError };
    match kind {
        IntegerKind::I8 | IntegerKind::I16 | IntegerKind::I32 => {
            quote! { #value.into_iter().map(|v| i64::from(v)).collect::<Vec<_>>() }
        }
        IntegerKind::I64 | IntegerKind::Isize => {
            quote! { #value.into_iter().map(|v| v as i64).collect::<Vec<_>>() }
        }
        IntegerKind::U8 | IntegerKind::U16 | IntegerKind::U32 => {
            quote! { #value.into_iter().map(|v| v as i64).collect::<Vec<_>>() }
        }
        IntegerKind::U64 | IntegerKind::Usize => {
            quote! {
                {
                    let value = #value;
                    let mut out = Vec::with_capacity(value.len());
                    for item in value.into_iter() {
                        let as_u64 = item as u64;
                        if as_u64 > i64::MAX as u64 {
                            return Err(#err::value_parse_owned(#section, #key, format!("value `{}` out of range for TOML integer", as_u64)));
                        }
                        out.push(as_u64 as i64);
                    }
                    out
                }
            }
        }
    }
}

/// Widens Rust float vectors into TOML float arrays.
fn float_vec_to_store(kind: &FloatKind, value: TokenStream) -> TokenStream {
    match kind {
        FloatKind::F64 => value,
        FloatKind::F32 => quote! { #value.into_iter().map(|v| f64::from(v)).collect::<Vec<_>>() },
    }
}
