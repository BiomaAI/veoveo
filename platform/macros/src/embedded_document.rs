//! Compile-time document bytes and SHA-256. The emitted include_str! also makes
//! rustc track the document as a build input when it changes.
use proc_macro::TokenStream;
use quote::quote;
use sha2::{Digest, Sha256};

pub fn expand(input: TokenStream) -> TokenStream {
    let relative = syn::parse_macro_input!(input as syn::LitStr);
    let result = (|| {
        let root =
            std::env::var("CARGO_MANIFEST_DIR").map_err(|_| "missing crate manifest directory")?;
        let path = std::path::Path::new(&root).join(relative.value());
        let bytes = std::fs::read(&path).map_err(|_| "cannot read embedded document")?;
        std::str::from_utf8(&bytes).map_err(|_| "embedded document must be UTF-8")?;
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        let path = path
            .to_str()
            .ok_or("embedded document path must be UTF-8")?;
        Ok::<_, &str>(quote! { (include_str!(#path), [#(#digest),*]) })
    })();
    match result {
        Ok(tokens) => tokens.into(),
        Err(message) => syn::Error::new(relative.span(), message)
            .to_compile_error()
            .into(),
    }
}
