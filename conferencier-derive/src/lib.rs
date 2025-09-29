mod codegen;
mod crate_path;
mod model;
mod parser;

use proc_macro::TokenStream;
use syn::DeriveInput;

/// Expands `#[derive(ConferModule)]` into load/save implementations driven by TOML metadata.
#[proc_macro_derive(ConferModule, attributes(confer))]
pub fn confer_module_derive(input: TokenStream) -> TokenStream {
    match expand(input) {
        Ok(tokens) => tokens,
        Err(err) => err.to_compile_error().into(),
    }
}

/// Parses the derive input and produces the final token stream.
fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    let input: DeriveInput = syn::parse(input)?;
    let module = parser::parse_module(input)?;
    let crate_path = crate_path::conferencier_path()?;
    let tokens = codegen::generate(module, crate_path)?;
    Ok(tokens.into())
}
