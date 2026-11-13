use std::collections::HashMap;

use proc_macro2::Span;
use quote::quote;
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{Attribute, DeriveInput, Expr, Field as SynField, Fields, LitStr, Result, Token, Type};

use crate::model::{
    ContainerKind, DeriveKind, Field, FieldDefault, FieldKind, FieldType, FloatKind, IntegerKind,
    ScalarKind, Scope, TableContainer, TableType,
};

/// Error reported for any use of the removed `#[confer(init)]` attribute.
const INIT_REMOVED: &str = "#[confer(init)] was removed in 0.0.4; use #[confer(ignore = <expr>)] for ignored fields; remove it from loaded fields and add #[confer(default = ...)] only if a missing-key fallback is intended";

/// Error reported for a `#[confer(table)]` field of an unsupported container shape.
const TABLE_SHAPE: &str = "#[confer(table)] supports T, Option<T> and Vec<T>";

/// Parses the derive input into the intermediate `Scope` representation.
pub fn parse_scope(input: DeriveInput, derive: DeriveKind) -> Result<Scope> {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = input;

    let (section, deny_unknown_keys) = parse_section_name(&attrs, &ident, derive)?;

    let data = match data {
        syn::Data::Struct(data) => data,
        _ => {
            return Err(syn::Error::new(
                ident.span(),
                format!(
                    "#[derive({})] can only be applied to structs",
                    derive.name()
                ),
            ));
        }
    };

    let fields = match data.fields {
        Fields::Named(named) => named.named,
        _ => {
            return Err(syn::Error::new(
                ident.span(),
                format!("#[derive({})] requires named fields", derive.name()),
            ));
        }
    };

    let mut result_fields = Vec::new();
    let mut seen_keys: HashMap<String, Span> = HashMap::new();

    for field in fields {
        result_fields.push(parse_field(&field, &mut seen_keys, derive)?);
    }

    Ok(Scope {
        ident,
        generics,
        section,
        deny_unknown_keys,
        fields: result_fields,
    })
}

/// Extracts the TOML section name and strictness from the `#[confer(...)]` attributes.
///
/// `ConfigTable` accepts and validates `section` but does not use it, so one struct can derive
/// both macros.
fn parse_section_name(
    attrs: &[Attribute],
    ident: &syn::Ident,
    derive: DeriveKind,
) -> Result<(String, bool)> {
    let mut section: Option<String> = None;
    let mut deny_unknown_keys = false;

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
            } else if meta.path.is_ident("deny_unknown_keys") {
                if deny_unknown_keys {
                    return Err(meta.error("duplicate #[confer(deny_unknown_keys)] attribute"));
                }
                if !meta.input.is_empty() && !meta.input.peek(Token![,]) {
                    return Err(meta.error("#[confer(deny_unknown_keys)] takes no value"));
                }
                deny_unknown_keys = true;
                Ok(())
            } else {
                Err(meta.error(format!(
                    "unsupported attribute on struct for #[derive({})]",
                    derive.name()
                )))
            }
        })?;
    }

    if let Some(section) = section {
        return Ok((section, deny_unknown_keys));
    }
    let name = ident.unraw().to_string();
    if matches!(derive, DeriveKind::Scope) {
        if let Some(old) = legacy_section_name(&name) {
            return Err(syn::Error::new(
                ident.span(),
                format!(
                    "`{name}` needs an explicit section: conferencier 0.0.4 stripped the leading \
                     `Confer` and used `[{old}]`, but the default section is now the full struct \
                     name `[{name}]`; add #[confer(section = \"{old}\")] to keep the old section \
                     or #[confer(section = \"{name}\")] to use the full name"
                ),
            ));
        }
    }
    Ok((name, deny_unknown_keys))
}

/// Parses an individual struct field, tracking duplicate keys and metadata.
fn parse_field(
    field: &SynField,
    seen_keys: &mut HashMap<String, Span>,
    derive: DeriveKind,
) -> Result<Field> {
    let ident = field
        .ident
        .clone()
        .ok_or_else(|| syn::Error::new(field.span(), "expected named field"))?;

    let mut rename: Option<(String, Span)> = None;
    let mut default: Option<FieldDefault> = None;
    let mut initializer: Option<Expr> = None;
    let mut ignore: Option<Span> = None;
    let mut table = false;

    for attr in &field.attrs {
        if !is_confer_attr(attr) {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                if rename.is_some() {
                    return Err(meta.error("duplicate #[confer(rename = ...)] attribute"));
                }
                let span = meta.path.span();
                let value: LitStr = meta.value()?.parse()?;
                rename = Some((value.value(), span));
                Ok(())
            } else if meta.path.is_ident("default") {
                if default.is_some() {
                    return Err(meta.error("duplicate #[confer(default)] attribute"));
                }
                let span = meta.path.span();
                let expr = if meta.input.peek(Token![=]) {
                    let expr: Expr = meta.value()?.parse()?;
                    Some(quote! { #expr })
                } else {
                    None
                };
                default = Some(FieldDefault { expr, span });
                Ok(())
            } else if meta.path.is_ident("init") {
                Err(meta.error(INIT_REMOVED))
            } else if meta.path.is_ident("ignore") {
                if ignore.is_some() {
                    return Err(meta.error("duplicate #[confer(ignore)] attribute"));
                }
                ignore = Some(meta.path.span());
                if meta.input.peek(Token![=]) {
                    initializer = Some(meta.value()?.parse()?);
                }
                Ok(())
            } else if meta.path.is_ident("table") {
                if table {
                    return Err(meta.error("duplicate #[confer(table)] attribute"));
                }
                table = true;
                Ok(())
            } else if meta.path.is_ident("deny_unknown_keys") {
                Err(meta.error("#[confer(deny_unknown_keys)] is a struct attribute"))
            } else {
                Err(meta.error(format!(
                    "unsupported attribute for #[derive({})]",
                    derive.name()
                )))
            }
        })?;
    }

    if table {
        if let Some(span) = ignore {
            return Err(syn::Error::new(
                span,
                "#[confer(table)] cannot be combined with #[confer(ignore)]",
            ));
        }
    }

    let ignore = ignore.is_some();
    if ignore {
        if let Some(default) = &default {
            return Err(syn::Error::new(
                default.span,
                "#[confer(default)] cannot be combined with #[confer(ignore)]; use #[confer(ignore = <expr>)]",
            ));
        }
        if let Some((_, span)) = &rename {
            return Err(syn::Error::new(
                *span,
                "#[confer(rename)] cannot be combined with #[confer(ignore)]; an ignored field has no TOML key",
            ));
        }
    }

    let key = rename.map_or_else(|| ident.unraw().to_string(), |(key, _)| key);

    // An ignored field has no TOML key, so it cannot collide with another field.
    if !ignore {
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
    }

    let kind = if ignore {
        None
    } else if table {
        Some(FieldKind::Table(Box::new(classify_table(&field.ty)?)))
    } else {
        Some(FieldKind::Scalar(classify_type(&field.ty, derive)?))
    };

    let is_option = match &kind {
        Some(FieldKind::Scalar(kind)) => {
            matches!(
                kind.container,
                ContainerKind::Option | ContainerKind::OptionVec
            )
        }
        Some(FieldKind::Table(kind)) => matches!(kind.container, TableContainer::Option),
        None => false,
    };
    if let Some(default) = &default {
        if is_option {
            return Err(syn::Error::new(
                default.span,
                "#[confer(default)] cannot be used on an Option field: a missing key already loads as None\nhelp: use the inner type with #[confer(default = ...)], or remove the attribute",
            ));
        }
    }

    let initializer_tokens = initializer.map(|expr| quote! { #expr });

    Ok(Field {
        ident,
        ty: field.ty.clone(),
        key,
        kind,
        default,
        initializer: initializer_tokens,
        ignore,
        span: field.span(),
    })
}

/// The section that conferencier 0.0.4 derived from `name`, if it differs from `name`.
///
/// 0.0.4 stripped a leading `Confer` only when it was a separate CamelCase word
/// (`ConferApp` -> `App`). The derive rejects such names without an explicit section, so the
/// change of the default cannot silently move a scope to another section (RFC-0005 N7).
// TODO(0.2.0): remove together with the guard in `parse_section_name`.
fn legacy_section_name(name: &str) -> Option<&str> {
    name.strip_prefix("Confer")
        .filter(|rest| rest.starts_with(char::is_uppercase))
}

/// Returns `true` when the attribute is `#[confer(...)]`.
fn is_confer_attr(attr: &Attribute) -> bool {
    attr.path().is_ident("confer")
}

/// Classifies a field type into container and scalar components.
fn classify_type(ty: &Type, derive: DeriveKind) -> Result<FieldType> {
    let (container, inner) = classify_container(ty)?;
    let scalar = classify_scalar(inner, derive)?;
    Ok(FieldType { container, scalar })
}

/// Classifies the type of a `#[confer(table)]` field into its container and inner type.
///
/// Only syntactically recognizable shapes are rejected; whether the inner type implements
/// `ConfigTable` is checked by the compiler on the generated code.
fn classify_table(ty: &Type) -> Result<TableType> {
    let ty = ungroup(ty);
    let (container, inner) = if let Some(inner) = match_outer_type(ty, "Option") {
        (TableContainer::Option, ungroup(inner))
    } else if let Some(inner) = match_outer_type(ty, "Vec") {
        (TableContainer::Vec, ungroup(inner))
    } else {
        (TableContainer::Plain, ty)
    };

    let supported = match inner {
        Type::Path(path) => {
            path.qself.is_some()
                || (match_outer_type(inner, "Option").is_none()
                    && match_outer_type(inner, "Vec").is_none())
        }
        Type::Macro(_) | Type::Verbatim(_) => true,
        _ => false,
    };
    if !supported {
        return Err(syn::Error::new(ty.span(), TABLE_SHAPE));
    }

    Ok(TableType {
        container,
        inner: inner.clone(),
    })
}

/// Strips invisible groups and parentheses around a type.
fn ungroup(mut ty: &Type) -> &Type {
    loop {
        match ty {
            Type::Group(group) => ty = &group.elem,
            Type::Paren(paren) => ty = &paren.elem,
            _ => return ty,
        }
    }
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
fn classify_scalar(ty: &Type, derive: DeriveKind) -> Result<ScalarKind> {
    let ident = type_ident(ty, derive)?;
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
            format!(
                "unsupported field type `{}`; nested structs require #[confer(table)] on the field and #[derive(ConfigTable)] on the type",
                other
            ),
        )),
    }
}

/// Extracts the terminal identifier from a type path.
fn type_ident(ty: &Type, derive: DeriveKind) -> Result<String> {
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
        Type::Reference(reference) => type_ident(&reference.elem, derive),
        _ => Err(syn::Error::new(
            ty.span(),
            format!("unsupported field type for #[derive({})]", derive.name()),
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
    if last.ident != expected {
        return None;
    }
    let syn::PathArguments::AngleBracketed(generic) = &last.arguments else {
        return None;
    };
    if generic.args.len() != 1 {
        return None;
    }

    match generic.args.first() {
        Some(syn::GenericArgument::Type(inner)) => Some(inner),
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    use super::legacy_section_name;

    #[test]
    fn legacy_names_strip_a_separate_confer_word() {
        assert_eq!(legacy_section_name("ConferApp"), Some("App"));
        assert_eq!(legacy_section_name("ConferConferApp"), Some("ConferApp"));
        assert_eq!(legacy_section_name("ConferÉtat"), Some("État"));
    }

    #[test]
    fn other_names_have_no_legacy_section() {
        assert_eq!(legacy_section_name("ConferenceSettings"), None);
        assert_eq!(legacy_section_name("Confer"), None);
        assert_eq!(legacy_section_name("Confer_App"), None);
        assert_eq!(legacy_section_name("Settings"), None);
    }
}
