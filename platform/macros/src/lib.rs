//! Thin compile-time declarations; runtime vocabulary semantics belong to veoveo-types.
use proc_macro::TokenStream;
mod embedded_document;
mod vocabulary;

#[proc_macro]
pub fn embedded_document(input: TokenStream) -> TokenStream {
    embedded_document::expand(input)
}

#[proc_macro_derive(Vocabulary, attributes(vocabulary, schemars))]
pub fn vocabulary(input: TokenStream) -> TokenStream {
    vocabulary::expand(input)
}
