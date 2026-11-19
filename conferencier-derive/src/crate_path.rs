use proc_macro2::Span;
use syn::{Error, Path};

/// Resolves the path to the `conferencier` crate, accommodating renamed dependencies.
///
/// A dependency found by Cargo is named with a leading `::`, so items named
/// `conferencier` in the deriving module cannot shadow it. `Itself` is reported
/// for doctests and examples of the core package, where the library is also an
/// extern crate. If the dependency is not found, the relative path
/// `conferencier` is resolved in the deriving module.
pub fn conferencier_path() -> Result<Path, Error> {
    match proc_macro_crate::crate_name("conferencier") {
        Ok(proc_macro_crate::FoundCrate::Itself) => Ok(syn::parse_quote!(::conferencier)),
        Ok(proc_macro_crate::FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, Span::call_site());
            Ok(syn::parse_quote!(::#ident))
        }
        Err(_) => Ok(syn::parse_quote!(conferencier)),
    }
}
