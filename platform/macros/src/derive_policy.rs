//! Standard derives supplied by the compact identity and address declarations.
use syn::{Attribute, Path, Token, punctuated::Punctuated};

pub(super) fn conflict(attribute: &Attribute, owner_derives: &[&str]) -> syn::Result<Option<Path>> {
    let derives = attribute.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)?;
    Ok(derives.into_iter().find(|path| {
        let name = path.segments.last().expect("derive path").ident.to_string();
        [
            "Clone",
            "Copy",
            "Debug",
            "PartialEq",
            "Eq",
            "PartialOrd",
            "Ord",
            "Hash",
            "Serialize",
            "Deserialize",
            "JsonSchema",
        ]
        .contains(&name.as_str())
            || owner_derives.contains(&name.as_str())
    }))
}
