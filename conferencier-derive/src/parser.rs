use std::collections::HashMap;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::meta::ParseNestedMeta;
use syn::spanned::Spanned;
use syn::{Attribute, DeriveInput, Expr, Field as SynField, Fields, Lit, LitStr, Result, Type};

use crate::model::{ContainerKind, Field, FieldType, FloatKind, IntegerKind, Module, ScalarKind};

/// Parses the derive input into the intermediate `Module` representation.
pub fn parse_module(input: DeriveInput) -> Result<Module> {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = input;

    let section = parse_section_name(&attrs, &ident)?;

    let data = match data {
        syn::Data::Struct(data) => data,
        _ => {
            return Err(syn::Error::new(
                ident.span(),
                "#[derive(ConferModule)] can only be applied to structs",
            ))
        }
    };

    let fields = match data.fields {
        Fields::Named(named) => named.named,
        _ => {
            return Err(syn::Error::new(
                ident.span(),
                "#[derive(ConferModule)] requires named fields",
            ))
        }
    };

    let mut result_fields = Vec::new();
    let mut seen_keys: HashMap<String, Span> = HashMap::new();

    for field in fields {
        result_fields.push(parse_field(&field, &mut seen_keys)?);
    }

    Ok(Module {
        ident,
        generics,
        section,
        fields: result_fields,
    })
}

/// Extracts the TOML section name from the `#[confer(...)]` attributes or generates a default.
fn parse_section_name(attrs: &[Attribute], ident: &syn::Ident) -> Result<String> {
    let mut section: Option<String> = None;

    for attr in attrs {
        if !is_confer_attr(attr) {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("section") {
                if section.is_some() {
                    return Err(meta.error("duplicate #[confer(section = ...)] attribute"));
                }
                let value: LitStr = meta.value()?.parse()?;
                section = Some(value.value());
                Ok(())
            } else {
                Err(meta.error("unsupported attribute on struct for #[derive(ConferModule)]"))
            }
        })?;
    }

    if let Some(section) = section {
        Ok(section)
    } else {
        Ok(default_section_name(ident))
    }
}

/// Parses an individual struct field, tracking duplicate keys and metadata.
fn parse_field(field: &SynField, seen_keys: &mut HashMap<String, Span>) -> Result<Field> {
    let ident = field
        .ident
        .clone()
        .ok_or_else(|| syn::Error::new(field.span(), "expected named field"))?;

    let mut rename: Option<String> = None;
    let mut default_expr: Option<Expr> = None;
    let mut init_expr: Option<Expr> = None;
    let mut ignore = false;

    for attr in &field.attrs {
        if !is_confer_attr(attr) {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                if rename.is_some() {
                    return Err(meta.error("duplicate #[confer(rename = ...)] attribute"));
                }
                let value: LitStr = meta.value()?.parse()?;
                rename = Some(value.value());
                Ok(())
            } else if meta.path.is_ident("default") {
                if default_expr.is_some() {
                    return Err(meta.error("duplicate #[confer(default = ...)] attribute"));
                }
                let expr: Expr = meta.value()?.parse()?;
                default_expr = Some(expr);
                Ok(())
            } else if meta.path.is_ident("init") {
                if init_expr.is_some() {
                    return Err(meta.error("duplicate #[confer(init = ...)] attribute"));
                }
                let expr: Expr = parse_init_expr(&meta)?;
                init_expr = Some(expr);
                Ok(())
            } else if meta.path.is_ident("ignore") {
                if ignore {
                    return Err(meta.error("duplicate #[confer(ignore)] attribute"));
                }
                ignore = true;
                Ok(())
            } else {
                Err(meta.error("unsupported attribute for #[derive(ConferModule)]"))
            }
        })?;
    }

    if default_expr.is_some() && init_expr.is_some() {
        return Err(syn::Error::new(
            field.span(),
            "#[confer(default = ...)] and #[confer(init = ...)] cannot be combined",
        ));
    }

    let key = rename.unwrap_or_else(|| ident.to_string());

    if let Some(prev_span) = seen_keys.insert(key.clone(), field.span()) {
        return Err(syn::Error::new(
            field.span(),
            format!(
                "duplicate TOML key `{}` detected (previously declared here)",
                key
            ),
        )
        .with_span(prev_span));
    }

    let kind = if ignore {
        None
    } else {
        Some(classify_type(&field.ty)?)
    };

    let default_tokens = if let (Some(expr), Some(kind)) = (&default_expr, &kind) {
        Some(transform_default(expr.clone(), kind)?)
    } else if let Some(expr) = &default_expr {
        Some(quote! { #expr })
    } else {
        None
    };

    let init_tokens = init_expr.map(|expr| quote! { #expr });

    Ok(Field {
        ident,
        key,
        kind,
        default: default_tokens,
        init: init_tokens,
        ignore,
        span: field.span(),
    })
}

/// Derives the default section name from the type identifier.
fn default_section_name(ident: &syn::Ident) -> String {
    let name = ident.to_string();
    if name.starts_with("Confer") {
        let trimmed = name.trim_start_matches("Confer").to_string();
        if trimmed.is_empty() {
            name
        } else {
            trimmed
        }
    } else {
        name
    }
}

/// Returns `true` when the attribute is `#[confer(...)]`.
fn is_confer_attr(attr: &Attribute) -> bool {
    attr.path().is_ident("confer")
}

/// Parses the `init` attribute, allowing either raw expressions or string literals.
fn parse_init_expr(meta: &ParseNestedMeta) -> Result<Expr> {
    let expr: Expr = meta.value()?.parse()?;
    if let Expr::Lit(expr_lit) = &expr {
        if let Lit::Str(lit) = &expr_lit.lit {
            return syn::parse_str::<Expr>(&lit.value());
        }
    }
    Ok(expr)
}

/// Classifies a field type into container and scalar components.
fn classify_type(ty: &Type) -> Result<FieldType> {
    let (container, inner) = classify_container(ty)?;
    let scalar = classify_scalar(inner)?;
    Ok(FieldType { container, scalar })
}

/// Determines the outer container kind and the innermost scalar type.
fn classify_container(ty: &Type) -> Result<(ContainerKind, &Type)> {
    if let Some(inner) = match_outer_type(ty, "Option") {
        let (inner_container, inner_ty) = classify_container(inner)?;
        return match inner_container {
            ContainerKind::Plain => Ok((ContainerKind::Option, inner_ty)),
            ContainerKind::Vec => Ok((ContainerKind::OptionVec, inner_ty)),
            _ => Err(syn::Error::new(
                inner.span(),
                "Option can only wrap scalar or Vec types",
            )),
        };
    }

    if let Some(inner) = match_outer_type(ty, "Vec") {
        return Ok((ContainerKind::Vec, inner));
    }

    Ok((ContainerKind::Plain, ty))
}

/// Resolves the scalar kind supported by the derive implementation.
fn classify_scalar(ty: &Type) -> Result<ScalarKind> {
    let ident = type_ident(ty)?;
    match ident.as_str() {
        "String" => Ok(ScalarKind::String),
        "bool" => Ok(ScalarKind::Bool),
        "i8" => Ok(ScalarKind::Integer(IntegerKind::I8)),
        "i16" => Ok(ScalarKind::Integer(IntegerKind::I16)),
        "i32" => Ok(ScalarKind::Integer(IntegerKind::I32)),
        "i64" => Ok(ScalarKind::Integer(IntegerKind::I64)),
        "isize" => Ok(ScalarKind::Integer(IntegerKind::Isize)),
        "u8" => Ok(ScalarKind::Integer(IntegerKind::U8)),
        "u16" => Ok(ScalarKind::Integer(IntegerKind::U16)),
        "u32" => Ok(ScalarKind::Integer(IntegerKind::U32)),
        "u64" => Ok(ScalarKind::Integer(IntegerKind::U64)),
        "usize" => Ok(ScalarKind::Integer(IntegerKind::Usize)),
        "f32" => Ok(ScalarKind::Float(FloatKind::F32)),
        "f64" => Ok(ScalarKind::Float(FloatKind::F64)),
        "Datetime" => Ok(ScalarKind::Datetime),
        other => Err(syn::Error::new(
            ty.span(),
            format!("unsupported field type `{}`", other),
        )),
    }
}

/// Extracts the terminal identifier from a type path.
fn type_ident(ty: &Type) -> Result<String> {
    match ty {
        Type::Path(path) if path.qself.is_none() => {
            let ident = &path
                .path
                .segments
                .last()
                .ok_or_else(|| syn::Error::new(ty.span(), "invalid type path"))?
                .ident;
            Ok(ident.to_string())
        }
        Type::Reference(reference) => type_ident(&reference.elem),
        _ => Err(syn::Error::new(
            ty.span(),
            "unsupported field type for #[derive(ConferModule)]",
        )),
    }
}

/// Returns the inner generic type when `ty` matches the expected outer type.
fn match_outer_type<'a>(ty: &'a Type, expected: &str) -> Option<&'a Type> {
    let path = match ty {
        Type::Path(path) if path.qself.is_none() => path,
        _ => return None,
    };

    let last = path.path.segments.last()?;
    if last.ident == expected {
        if let syn::PathArguments::AngleBracketed(generic) = &last.arguments {
            if generic.args.len() == 1 {
                if let syn::GenericArgument::Type(inner) = generic.args.first().unwrap() {
                    return Some(inner);
                }
            }
        }
    }

    None
}

/// Converts a literal default expression into tokens matching the field type.
fn transform_default(expr: Expr, field_type: &FieldType) -> Result<TokenStream> {
    match field_type.container {
        ContainerKind::Plain => transform_plain_default(expr, &field_type.scalar),
        ContainerKind::Vec => transform_vec_default(expr, &field_type.scalar, false),
        ContainerKind::Option => transform_option_default(expr, &field_type.scalar),
        ContainerKind::OptionVec => transform_vec_default(expr, &field_type.scalar, true),
    }
}

/// Handles defaults for non-container fields.
fn transform_plain_default(expr: Expr, scalar: &ScalarKind) -> Result<TokenStream> {
    literal_tokens(expr, scalar)
}

/// Wraps defaults for optional fields in `Some(...)`.
fn transform_option_default(expr: Expr, scalar: &ScalarKind) -> Result<TokenStream> {
    let tokens = literal_tokens(expr, scalar)?;
    Ok(quote! { Some(#tokens) })
}

/// Validates and converts defaults specified for vector and option-vector fields.
fn transform_vec_default(
    expr: Expr,
    scalar: &ScalarKind,
    wrap_option: bool,
) -> Result<TokenStream> {
    match expr {
        Expr::Array(array) => {
            let elements: Vec<_> = array
                .elems
                .into_iter()
                .map(|element| literal_tokens(element, scalar))
                .collect::<Result<Vec<_>>>()?;
            if wrap_option {
                Ok(quote! { Some(vec![#(#elements),*]) })
            } else {
                Ok(quote! { vec![#(#elements),*] })
            }
        }
        _ => Err(syn::Error::new(
            expr.span(),
            "defaults for Vec<T> must use [ ... ] syntax",
        )),
    }
}

/// Ensures the provided literal matches the scalar kind expected by the field.
fn validate_literal(expr: &Expr, scalar: &ScalarKind) -> Result<()> {
    match scalar {
        ScalarKind::String | ScalarKind::Datetime => match expr {
            Expr::Lit(expr_lit) => match &expr_lit.lit {
                Lit::Str(_) => Ok(()),
                _ => Err(syn::Error::new(
                    expr.span(),
                    "expected string literal",
                )),
            },
            _ => Err(syn::Error::new(expr.span(), "expected string literal")),
        },
        ScalarKind::Bool => match expr {
            Expr::Lit(expr_lit) => match expr_lit.lit {
                Lit::Bool(_) => Ok(()),
                _ => Err(syn::Error::new(expr.span(), "expected boolean literal")),
            },
            _ => Err(syn::Error::new(expr.span(), "expected boolean literal")),
        },
        ScalarKind::Integer(_) => match expr {
            Expr::Lit(expr_lit) => match expr_lit.lit {
                Lit::Int(_) => Ok(()),
                _ => Err(syn::Error::new(expr.span(), "expected integer literal")),
            },
            _ => Err(syn::Error::new(expr.span(), "expected integer literal")),
        },
        ScalarKind::Float(_) => match expr {
            Expr::Lit(expr_lit) => match expr_lit.lit {
                Lit::Float(_) | Lit::Int(_) => Ok(()),
                _ => Err(syn::Error::new(expr.span(), "expected float literal")),
            },
            _ => Err(syn::Error::new(expr.span(), "expected float literal")),
        },
    }
}

/// Transforms validated literals into concrete tokens used in generated code.
fn literal_tokens(expr: Expr, scalar: &ScalarKind) -> Result<TokenStream> {
    validate_literal(&expr, scalar)?;
    Ok(match scalar {
        ScalarKind::String => quote! { (#expr).to_string() },
        ScalarKind::Bool => quote! { #expr },
        ScalarKind::Integer(_) => quote! { #expr },
        ScalarKind::Float(kind) => {
            let ty = kind.type_tokens();
            quote! { (#expr) as #ty }
        }
        ScalarKind::Datetime => {
            quote! { <toml::value::Datetime as std::str::FromStr>::from_str(#expr).expect("invalid datetime literal") }
        }
    })
}

/// Helper trait for enriching errors with additional span information.
trait ErrorWithSpan {
    /// Combines `self` with an extra error pointing at `span` for better diagnostics.
    fn with_span(self, span: Span) -> Self;
}

impl ErrorWithSpan for syn::Error {
    fn with_span(mut self, span: Span) -> Self {
        self.combine(syn::Error::new(span, "see previous definition"));
        self
    }
}
