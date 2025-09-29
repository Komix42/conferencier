use proc_macro2::Span;
use syn::{Error, Path};

/// Resolves the path to the `conferencier` crate, accommodating renamed dependencies.
pub fn conferencier_path() -> Result<Path, Error> {
    match proc_macro_crate::crate_name("conferencier") {
        Ok(proc_macro_crate::FoundCrate::Itself) => Ok(syn::parse_quote!(conferencier)),
        Ok(proc_macro_crate::FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, Span::call_site());
            let mut path = Path::from(ident);
            path.leading_colon = None;
            Ok(path)
        }
        Err(_) => Ok(syn::parse_quote!(conferencier)),
    }
}
