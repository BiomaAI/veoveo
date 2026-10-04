//! Thin compile-time declarations; runtime vocabulary semantics belong to veoveo-types.
use proc_macro::TokenStream;
mod embedded_document;
mod id;
mod id_frontend;
mod resource_address;
mod resource_address_frontend;
mod vocabulary;

#[proc_macro]
pub fn embedded_document(input: TokenStream) -> TokenStream {
    embedded_document::expand(input)
}

#[proc_macro_derive(Vocabulary, attributes(vocabulary, schemars))]
pub fn vocabulary(input: TokenStream) -> TokenStream {
    vocabulary::expand(input)
}

#[proc_macro_attribute]
pub fn id(arguments: TokenStream, item: TokenStream) -> TokenStream {
    id_frontend::expand(arguments.into(), item.into())
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

#[proc_macro_attribute]
pub fn resource_address(arguments: TokenStream, item: TokenStream) -> TokenStream {
    resource_address_frontend::expand(arguments.into(), item.into())
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}
