mod codegen;
mod crate_path;
mod model;
mod parser;

use proc_macro::TokenStream;
use syn::DeriveInput;

use crate::model::DeriveKind;

/// Expands `#[derive(ConferModule)]` into load/save implementations driven by TOML metadata.
#[proc_macro_derive(ConferModule, attributes(confer))]
pub fn confer_module_derive(input: TokenStream) -> TokenStream {
    expand(input, DeriveKind::Module)
}

/// Expands `#[derive(ConferTable)]` into conversions between the struct and a nested TOML table.
#[proc_macro_derive(ConferTable, attributes(confer))]
pub fn confer_table_derive(input: TokenStream) -> TokenStream {
    expand(input, DeriveKind::Table)
}

/// Runs the derive and turns errors into compile errors.
fn expand(input: TokenStream, derive: DeriveKind) -> TokenStream {
    match try_expand(input, derive) {
        Ok(tokens) => tokens,
        Err(err) => err.to_compile_error().into(),
    }
}

/// Parses the derive input and produces the final token stream.
fn try_expand(input: TokenStream, derive: DeriveKind) -> syn::Result<TokenStream> {
    let input: DeriveInput = syn::parse(input)?;
    let module = parser::parse_module(input, derive)?;
    let crate_path = crate_path::conferencier_path()?;
    let tokens = match derive {
        DeriveKind::Module => codegen::generate(module, crate_path)?,
        DeriveKind::Table => codegen::generate_table(module, crate_path)?,
    };
    Ok(tokens.into())
}
