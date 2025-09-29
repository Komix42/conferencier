//! Helpers for converting TOML values into strongly typed Rust values.

use std::str::FromStr;

use toml::value::Datetime;
use toml::Value;

use crate::error::{ConferError, Result};

/// Human-readable description of a TOML [`Value`] type.
pub fn describe(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Integer(_) => "integer",
        Value::Float(_) => "float",
        Value::Boolean(_) => "boolean",
        Value::Datetime(_) => "datetime",
        Value::Array(_) => "array",
        Value::Table(_) => "table",
    }
}

/// Converts a TOML value to `String`, producing a type-mismatch error when incompatible.
pub fn string(section: &str, key: &str, value: Value) -> Result<String> {
    match value {
        Value::String(s) => Ok(s),
        other => Err(ConferError::type_mismatch(section, key, "string", describe(&other))),
    }
}

/// Converts a TOML value to `i64`, producing a type-mismatch error when incompatible.
pub fn integer(section: &str, key: &str, value: Value) -> Result<i64> {
    match value {
        Value::Integer(v) => Ok(v),
        other => Err(ConferError::type_mismatch(section, key, "integer", describe(&other))),
    }
}

/// Converts a TOML value to `f64`, accepting integers and floats.
pub fn float(section: &str, key: &str, value: Value) -> Result<f64> {
    match value {
        Value::Float(v) => Ok(v),
        Value::Integer(v) => Ok(v as f64),
        other => Err(ConferError::type_mismatch(section, key, "float", describe(&other))),
    }
}

/// Converts a TOML value to `bool`, producing a type-mismatch error when incompatible.
pub fn boolean(section: &str, key: &str, value: Value) -> Result<bool> {
    match value {
        Value::Boolean(v) => Ok(v),
        other => Err(ConferError::type_mismatch(section, key, "boolean", describe(&other))),
    }
}

/// Converts a TOML value to [`Datetime`], parsing strings when necessary.
pub fn datetime(section: &str, key: &str, value: Value) -> Result<Datetime> {
    match value {
        Value::Datetime(dt) => Ok(dt),
        Value::String(s) => parse_datetime(section, key, &s),
        other => Err(ConferError::type_mismatch(section, key, "datetime", describe(&other))),
    }
}

/// Converts a TOML value to `Vec<String>`, validating element types.
pub fn string_vec(section: &str, key: &str, value: Value) -> Result<Vec<String>> {
    to_vec(section, key, value, |section, key, element| match element {
        Value::String(s) => Ok(s),
        other => Err(element_mismatch(section, key, "string", &other)),
    })
}

/// Converts a TOML value to `Vec<i64>`, validating element types.
pub fn integer_vec(section: &str, key: &str, value: Value) -> Result<Vec<i64>> {
    to_vec(section, key, value, |section, key, element| match element {
        Value::Integer(v) => Ok(v),
        other => Err(element_mismatch(section, key, "integer", &other)),
    })
}

/// Converts a TOML value to `Vec<f64>`, upcasting integer elements when needed.
pub fn float_vec(section: &str, key: &str, value: Value) -> Result<Vec<f64>> {
    to_vec(section, key, value, |section, key, element| match element {
        Value::Float(v) => Ok(v),
        Value::Integer(v) => Ok(v as f64),
        other => Err(element_mismatch(section, key, "float", &other)),
    })
}

/// Converts a TOML value to `Vec<bool>`, validating element types.
pub fn boolean_vec(section: &str, key: &str, value: Value) -> Result<Vec<bool>> {
    to_vec(section, key, value, |section, key, element| match element {
        Value::Boolean(v) => Ok(v),
        other => Err(element_mismatch(section, key, "boolean", &other)),
    })
}

/// Converts a TOML value to `Vec<Datetime>`, parsing string elements when necessary.
pub fn datetime_vec(section: &str, key: &str, value: Value) -> Result<Vec<Datetime>> {
    to_vec(section, key, value, |section, key, element| match element {
        Value::Datetime(dt) => Ok(dt),
        Value::String(s) => parse_datetime(section, key, &s),
        other => Err(element_mismatch(section, key, "datetime", &other)),
    })
}

/// Parses a TOML datetime from `raw`, annotating errors with section/key context.
fn parse_datetime(section: &str, key: &str, raw: &str) -> Result<Datetime> {
    Datetime::from_str(raw).map_err(|err| {
        ConferError::value_parse(section, key, format!("failed to parse datetime: {err}"))
    })
}

/// Converts a TOML array to `Vec<T>` using the provided element conversion callback.
fn to_vec<T, F>(section: &str, key: &str, value: Value, mut convert: F) -> Result<Vec<T>>
where
    F: FnMut(&str, &str, Value) -> Result<T>,
{
    match value {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (index, item) in items.into_iter().enumerate() {
                match convert(section, key, item) {
                    Ok(v) => out.push(v),
                    Err(err) => {
                        return Err(match err {
                            ConferError::TypeMismatch { .. } | ConferError::ValueParse { .. } => {
                                annotate_with_index(err, index)
                            }
                            other => other,
                        });
                    }
                }
            }
            Ok(out)
        }
        other => Err(ConferError::type_mismatch(section, key, "array", describe(&other))),
    }
}

/// Builds a [`ConferError::ValueParse`] describing an invalid array element type.
fn element_mismatch(section: &str, key: &str, expected: &'static str, value: &Value) -> ConferError {
    ConferError::value_parse(
        section,
        key,
        format!(
            "expected array elements of type {expected}, found {}",
            describe(value)
        ),
    )
}

/// Adds index context to element-related errors to aid debugging.
fn annotate_with_index(error: ConferError, index: usize) -> ConferError {
    match error {
        ConferError::ValueParse { section, key, message } => ConferError::ValueParse {
            section,
            key,
            message: format!("{message} (at index {index})"),
        },
        ConferError::TypeMismatch { section, key, expected, found } => ConferError::TypeMismatch {
            section,
            key,
            expected,
            found,
        },
        other => other,
    }
}
