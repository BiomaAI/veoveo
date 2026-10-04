use super::id::{Expansion, emit};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Data, DeriveInput, Expr, Fields, Ident, LitStr, Path, Token,
    parse::{Parse, ParseStream},
};

struct Declaration {
    storage: Ident,
    profile: Option<Path>,
    custom: Option<TokenStream>,
    prefix: LitStr,
    context: Option<Expr>,
    fresh: bool,
    stable: bool,
    constant: bool,
    secret: bool,
    ordered: bool,
    inner_display: bool,
    surreal: Option<LitStr>,
}
impl Parse for Declaration {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let storage: Ident = input.parse()?;
        if !["text", "hex", "prefixed", "uuid", "custom"].contains(&storage.to_string().as_str()) {
            return Err(syn::Error::new_spanned(
                storage,
                "expected text, hex, prefixed, uuid or custom form",
            ));
        }
        let body;
        syn::parenthesized!(body in input);
        let custom = (storage == "custom").then(|| body.parse()).transpose()?;
        let profile = if custom.is_none() {
            Some(body.parse()?)
        } else {
            None
        };
        let prefix = if storage == "prefixed" {
            body.parse::<Token![,]>()?;
            body.parse()?
        } else {
            LitStr::new("", input.span())
        };
        if !body.is_empty() {
            return Err(body.error("unexpected identity form arguments"));
        }
        let mut value = Self {
            storage,
            profile,
            custom,
            prefix,
            context: None,
            fresh: false,
            stable: false,
            constant: false,
            secret: false,
            ordered: false,
            inner_display: false,
            surreal: None,
        };
        let mut seen = std::collections::BTreeSet::new();
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let option: Ident = input.parse()?;
            if !seen.insert(option.to_string()) {
                return Err(syn::Error::new_spanned(
                    option,
                    "duplicate identity capability",
                ));
            }
            match option.to_string().as_str() {
                "fresh" => value.fresh = true,
                "stable" => value.stable = true,
                "const_uuid" => value.constant = true,
                "secret" => value.secret = true,
                "ordered" => value.ordered = true,
                "inner_display" => value.inner_display = true,
                "prefix" => {
                    input.parse::<Token![=]>()?;
                    value.prefix = input.parse()?;
                }
                "error_context" => {
                    input.parse::<Token![=]>()?;
                    value.context = Some(input.parse()?);
                }
                "surreal" => {
                    input.parse::<Token![=]>()?;
                    value.surreal = Some(input.parse()?);
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        option,
                        "unknown identity capability",
                    ));
                }
            }
        }
        if value.ordered && !value.secret
            || value.inner_display && value.secret
            || value.constant && value.storage != "uuid"
            || value.surreal.is_some() && !value.constant
            || value.storage == "custom" && !seen.is_empty()
        {
            return Err(syn::Error::new_spanned(
                &value.storage,
                "identity capability does not match storage",
            ));
        }
        if let Some(table) = &value.surreal {
            let name = table.value();
            if name.is_empty()
                || name.len() > 128
                || !name.as_bytes()[0].is_ascii_lowercase()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err(syn::Error::new_spanned(
                    table,
                    "surreal table requires a lowercase ASCII table identifier",
                ));
            }
        }
        Ok(value)
    }
}
// Negative Serde-conversion directives were consumed with the owner declaration.
// The generated helper has no conversion attributes for them to override.
fn schema_metadata(mut attribute: syn::Attribute) -> syn::Attribute {
    if attribute.path().is_ident("schemars")
        && let syn::Meta::List(list) = &mut attribute.meta
    {
        let tokens: Vec<_> = list.tokens.clone().into_iter().collect();
        let mut kept = Vec::new();
        let mut index = 0;
        while index < tokens.len() {
            if matches!(&tokens[index], proc_macro2::TokenTree::Punct(p) if p.as_char() == '!')
                    && tokens.get(index+1).is_some_and(|t| matches!(t, proc_macro2::TokenTree::Ident(i) if i == "into" || i == "try_from")) {
                    index += 2;
                    if tokens.get(index).is_some_and(|t| matches!(t, proc_macro2::TokenTree::Punct(p) if p.as_char() == ',')) { index += 1; }
                } else { kept.push(tokens[index].clone()); index += 1; }
        }
        list.tokens = kept.into_iter().collect();
    }
    attribute
}
pub(super) fn expand(arguments: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let declaration: Declaration = syn::parse2(arguments)?;
    let mut input: DeriveInput = syn::parse2(item)?;
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input,
            "identity requires a nongeneric one-field tuple struct",
        ));
    };
    let Fields::Unnamed(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input,
            "identity requires tuple storage",
        ));
    };
    if fields.unnamed.len() != 1
        || !input.generics.params.is_empty()
        || input.generics.where_clause.is_some()
    {
        return Err(syn::Error::new_spanned(
            &input,
            "identity requires a nongeneric one-field tuple struct",
        ));
    }
    if let Some(custom) = declaration.custom {
        let generated = emit(&input, Expansion::custom(custom)?);
        return Ok(quote!(#input #generated));
    }
    let inner = &fields.unnamed[0].ty;
    let uuid = matches!(inner, syn::Type::Path(path) if path.path.segments.last().is_some_and(|s| s.ident == "Uuid"));
    let expected = if uuid { "Uuid" } else { "String" };
    if uuid && declaration.storage != "uuid"
        || declaration.constant && !uuid
        || declaration.stable && uuid
        || declaration.secret && uuid
    {
        return Err(syn::Error::new_spanned(
            inner,
            "identity capability does not match storage",
        ));
    }
    if !matches!(inner, syn::Type::Path(path) if path.path.segments.last().is_some_and(|s| s.ident == expected))
    {
        return Err(syn::Error::new_spanned(
            inner,
            format!("storage requires {expected}"),
        ));
    }
    let field_schema: Vec<_> = fields.unnamed[0]
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("doc") || a.path().is_ident("schemars"))
        .cloned()
        .collect();
    for attribute in &input.attrs {
        if attribute.path().is_ident("serde") {
            return Err(syn::Error::new_spanned(attribute, "profile owns Serde"));
        }
        if attribute.path().is_ident("derive") {
            let syn::Meta::List(list) = &attribute.meta else {
                continue;
            };
            let derives = list.parse_args_with(
                syn::punctuated::Punctuated::<Path, Token![,]>::parse_terminated,
            )?;
            for derive in derives {
                let last = derive.segments.last().unwrap().ident.to_string();
                if [
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
                    "Id",
                ]
                .contains(&last.as_str())
                    || declaration.surreal.is_some() && last == "SurrealValue"
                {
                    return Err(syn::Error::new_spanned(
                        derive,
                        "identity owns standard derives",
                    ));
                }
            }
        }
    }
    let schema_attrs: Vec<_> = input
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("doc") || a.path().is_ident("schemars"))
        .cloned()
        .map(schema_metadata)
        .collect();
    let rename = !schema_attrs.iter().any(|a| {
        a.path().is_ident("schemars") && matches!(&a.meta, syn::Meta::List(list) if {
            let tokens: Vec<_> = list.tokens.clone().into_iter().collect();
            tokens.windows(2).any(|pair| matches!(&pair[0], proc_macro2::TokenTree::Ident(i) if i == "rename") && matches!(&pair[1], proc_macro2::TokenTree::Punct(p) if p.as_char() == '='))
        })
    });
    input.attrs.retain(|a| !a.path().is_ident("schemars"));
    if let Data::Struct(data) = &mut input.data {
        for field in &mut data.fields {
            field.attrs.retain(|a| !a.path().is_ident("schemars"));
        }
    }
    let name = &input.ident;
    let profile = declaration.profile.unwrap();
    let prefix = declaration.prefix;
    let context = declaration
        .context
        .map(|e| quote!(#e))
        .unwrap_or_else(|| quote!(stringify!(#name)));
    let metadata = quote!(::veoveo_types::IdMetadata { type_name: stringify!(#name), prefix: #prefix, error_context: #context });
    let policy = quote!(<#profile as ::veoveo_types::IdProfile>::PROFILE);
    let error = syn::parse_quote!(<#profile as ::veoveo_types::IdProfile>::Error);
    let admit = if uuid {
        syn::parse_quote!(|value| ::veoveo_types::admit_profile_uuid::<#profile>(value, #metadata))
    } else {
        syn::parse_quote!(|value| ::veoveo_types::admit_profile_string::<#profile>(value, #metadata))
    };
    let fresh = declaration.fresh.then(|| {
        if uuid {
            syn::parse_quote!(|| ::veoveo_types::fresh_profile_uuid::<#profile>(#metadata))
        } else {
            syn::parse_quote!(|| ::veoveo_types::fresh_profile_string::<#profile>(#metadata))
        }
    });
    let traits = if declaration.secret && !declaration.ordered {
        quote!(#[derive(Clone, PartialEq, Eq)])
    } else if declaration.secret {
        quote!(#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)])
    } else if uuid {
        quote!(#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)])
    } else {
        quote!(#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)])
    };
    let expected_grammar = match declaration.storage.to_string().as_str() {
        "text" => Some(quote!(::veoveo_types::IdGrammar::Text(_))),
        "hex" => Some(quote!(::veoveo_types::IdGrammar::Hex(_, _))),
        "uuid" => Some(quote!(::veoveo_types::IdGrammar::Uuid(_, _))),
        _ => None,
    };
    let check_uuid = expected_grammar.map(|pattern| quote!(assert!(matches!(#policy.grammar, #pattern), "identity form requires matching owner admission" );));
    let check_fresh = declaration.fresh.then(|| quote!(assert!(matches!(#policy.generation.fresh, ::veoveo_types::FreshId::UuidV7) && ::veoveo_types::uuid_grammar_accepts_version(#policy.grammar, 7), "fresh UUIDv7 generation must be admitted");));
    let check_stable = declaration.stable.then(|| quote!(assert!(#policy.generation.stable_v5_namespace.is_some() && ::veoveo_types::uuid_grammar_accepts_version(#policy.grammar, 5), "stable UUIDv5 generation must be admitted");));
    let constant = declaration.constant.then(|| quote! {
        const _: () = { match #policy.grammar {
            ::veoveo_types::IdGrammar::Uuid(g, _) => assert!(g.versions.is_empty() && matches!(g.variant, ::veoveo_types::UuidVariant::Any) && matches!(g.spelling, ::veoveo_types::UuidSpelling::ParserAliases), "const UUID construction requires unrestricted admission"),
            _ => panic!("const UUID construction requires UUID grammar"),
        } };
        impl #name { pub const fn from_uuid(value: ::uuid::Uuid) -> Self { Self(value) } }
    });
    let uuid_api = uuid.then(|| quote! {
        impl #name { pub const fn as_uuid(self) -> ::uuid::Uuid { self.0 } }
        impl ::veoveo_types::UuidIdentity for #name { fn as_uuid(&self) -> ::uuid::Uuid { self.0 } }
        impl From<#name> for ::uuid::Uuid { fn from(value: #name) -> Self { value.0 } }
        impl TryFrom<::uuid::Uuid> for #name {
            type Error = <#profile as ::veoveo_types::IdProfile>::Error;
            fn try_from(value: ::uuid::Uuid) -> Result<Self, Self::Error> { Self::parse(value.to_string()) }
        }
        impl TryFrom<::std::string::String> for #name {
            type Error = <#profile as ::veoveo_types::IdProfile>::Error;
            fn try_from(value: ::std::string::String) -> Result<Self, Self::Error> { Self::parse(value) }
        }
    });
    let string_uuid_api = (!uuid && (declaration.storage == "uuid" || declaration.fresh || declaration.stable)).then(|| quote! {
        impl ::veoveo_types::UuidIdentity for #name {
            fn as_uuid(&self) -> ::uuid::Uuid { ::veoveo_types::admit_profile_uuid::<#profile>(&self.0, #metadata).unwrap_or_else(|_| panic!("identity storage preserves admission")) }
        }
        impl #name { pub fn as_uuid(&self) -> ::uuid::Uuid { <Self as ::veoveo_types::UuidIdentity>::as_uuid(self) } }
    });
    let stable = declaration.stable.then(|| quote! {
        impl ::veoveo_types::StableKeyIdentity for #name { fn from_stable_key(key: &[u8]) -> Self { Self(::veoveo_types::stable_profile_string::<#profile>(key, #metadata)) } }
        impl #name { pub fn from_stable_key(key: &[u8]) -> Self { <Self as ::veoveo_types::StableKeyIdentity>::from_stable_key(key) } }
    });
    let sdk = declaration.surreal.as_ref().map(|table| quote! {
        impl #name { pub const TABLE: &'static str = #table; pub fn record_id(self) -> ::surrealdb::types::RecordId { ::surrealdb::types::RecordId::new(Self::TABLE, ::surrealdb::types::Uuid::from(self.0)) } }
        impl From<#name> for ::surrealdb::types::RecordId { fn from(value: #name) -> Self { value.record_id() } }
    });
    let sdk_derive = declaration
        .surreal
        .map(|_| quote!(#[derive(::surrealdb::types::SurrealValue)]));
    let serialized = if uuid {
        quote!(match #policy.wire { ::veoveo_types::IdWire::String => serializer.serialize_str(&self.0.to_string()), _ => ::serde::Serialize::serialize(&self.0, serializer) })
    } else {
        quote!(serializer.serialize_str(&self.0))
    };
    let deserialized = if uuid {
        quote!(match #policy.wire {
            ::veoveo_types::IdWire::InnerUuid => { let value = <::uuid::Uuid as ::serde::Deserialize>::deserialize(deserializer)?; Self::try_from(value).map_err(::serde::de::Error::custom) },
            _ => { let value = <::std::string::String as ::serde::Deserialize>::deserialize(deserializer)?; Self::try_from(value).map_err(::serde::de::Error::custom) }
        })
    } else {
        quote! { let value = <::std::string::String as ::serde::Deserialize>::deserialize(deserializer)?; Self::try_from(value).map_err(::serde::de::Error::custom) }
    };
    let schema_name = LitStr::new(&name.to_string(), name.span());
    let default_rename = rename.then(|| quote!(#[schemars(rename = #schema_name)]));
    let schema_uuid =
        uuid || declaration.storage == "uuid" || declaration.fresh || declaration.stable;
    let uuid_helper = schema_uuid.then(|| quote! { #[allow(dead_code)] #[derive(::schemars::JsonSchema)] #[schemars(transparent)] #default_rename #(#schema_attrs)* struct __DerivedUuid(#(#field_schema)* ::uuid::Uuid); });
    let uuid_type = if schema_uuid {
        quote!(__DerivedUuid)
    } else {
        quote!(__DerivedString)
    };
    let check_wire = (!uuid).then(|| quote!(assert!(matches!(#policy.wire, ::veoveo_types::IdWire::String), "String storage requires String wire" );));
    let prefix_api = (declaration.storage == "prefixed").then(|| {
        quote! { impl #name { pub const PREFIX: &'static str = #prefix; } }
    });
    let extra = quote! {
        const _: () = { #check_uuid #check_fresh #check_stable #check_wire };
        #constant #prefix_api #uuid_api #string_uuid_api #stable #sdk
        impl ::serde::Serialize for #name { fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> { #serialized } }
        impl<'de> ::serde::Deserialize<'de> for #name { fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> { #deserialized } }
        const _: () = {
            #[allow(dead_code)] #[derive(::schemars::JsonSchema)] #default_rename #(#schema_attrs)* struct __DerivedString(#(#field_schema)* String);
            #uuid_helper
            impl ::schemars::JsonSchema for #name {
                fn schema_name() -> ::std::borrow::Cow<'static,str> { match #policy.schema { ::veoveo_types::IdSchema::DerivedString => <__DerivedString as ::schemars::JsonSchema>::schema_name(), ::veoveo_types::IdSchema::DerivedUuid => <#uuid_type as ::schemars::JsonSchema>::schema_name(), _ => stringify!(#name).into() } }
                fn schema_id() -> ::std::borrow::Cow<'static,str> { if let Some(id) = #policy.schema_id { return id(#metadata); } match #policy.schema { ::veoveo_types::IdSchema::DerivedString => <__DerivedString as ::schemars::JsonSchema>::schema_id(), ::veoveo_types::IdSchema::DerivedUuid => <#uuid_type as ::schemars::JsonSchema>::schema_id(), _ => Self::schema_name() } }
                fn inline_schema() -> bool { match #policy.schema { ::veoveo_types::IdSchema::DerivedString => <__DerivedString as ::schemars::JsonSchema>::inline_schema(), ::veoveo_types::IdSchema::DerivedUuid => <#uuid_type as ::schemars::JsonSchema>::inline_schema(), ::veoveo_types::IdSchema::Owner { inline, .. } => inline, _ => false } }
                fn json_schema(generator: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema { match #policy.schema {
                    ::veoveo_types::IdSchema::DerivedString => <__DerivedString as ::schemars::JsonSchema>::json_schema(generator),
                    ::veoveo_types::IdSchema::DerivedUuid => <#uuid_type as ::schemars::JsonSchema>::json_schema(generator),
                    ::veoveo_types::IdSchema::Owner { schema, .. } => schema(generator, #metadata),
                    ::veoveo_types::IdSchema::UuidPattern { pattern } => { let mut schema = <#uuid_type as ::schemars::JsonSchema>::json_schema(generator); let object = schema.ensure_object(); object.insert("pattern".into(), pattern.into()); object.insert("minLength".into(),36.into()); object.insert("maxLength".into(),36.into()); schema },
                    ::veoveo_types::IdSchema::Hex { pattern } => { let mut schema = <__DerivedString as ::schemars::JsonSchema>::json_schema(generator); schema.ensure_object().insert("pattern".into(),pattern.into()); schema },
                } }
            }
        };
    };
    let generated = emit(
        &input,
        Expansion {
            error,
            admit: Some(admit),
            owned_admit: (!uuid).then(|| syn::parse_quote!(|value| ::veoveo_types::admit_profile_owned_string::<#profile>(value, #metadata))),
            validate: None,
            text: None,
            string: !uuid,
            project: !declaration.secret,
            wire_string: false,
            generate: fresh,
            schema: None,
            schema_inline: false,
            display: !declaration.secret,
            inner_display: declaration.inner_display,
            extra,
        },
    );
    Ok(quote!(#sdk_derive #traits #input #generated))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_forms_and_explicit_exceptions_share_the_emitter() {
        for (arguments, item) in [
            (
                quote!(text(Names)),
                quote!(
                    struct Name(String);
                ),
            ),
            (
                quote!(hex(Digests)),
                quote!(
                    struct Digest(String);
                ),
            ),
            (
                quote!(prefixed(MapIds, "route-"), fresh, stable),
                quote!(
                    struct RouteId(String);
                ),
            ),
            (
                quote!(uuid(Requests), fresh),
                quote!(
                    struct RequestId(uuid::Uuid);
                ),
            ),
            (
                quote!(uuid(Aliases)),
                quote!(
                    struct StreamId(String);
                ),
            ),
            (
                quote!(
                    uuid(StoreIds),
                    fresh,
                    const_uuid,
                    surreal = "fixture_identity"
                ),
                quote!(
                    struct RowId(uuid::Uuid);
                ),
            ),
            (
                quote!(text(Secrets), secret, ordered),
                quote!(
                    struct Secret(String);
                ),
            ),
            (
                quote!(custom(error = Error, admit = parse_task, text = task_text)),
                quote!(
                    struct RunId(TaskId);
                ),
            ),
        ] {
            let output = expand(arguments, item).unwrap().to_string();
            assert!(output.contains("parse_identity"));
            assert!(output.contains("fn parse"));
            assert!(!output.contains("# [id"));
        }
    }
    #[test]
    fn malformed_declarations_diagnose_the_actual_frontend() {
        for (arguments, item, message) in [
            (
                quote!(text(Names, ignored)),
                quote!(
                    struct Name(String);
                ),
                "unexpected identity form arguments",
            ),
            (
                quote!(text(Names), fresh, fresh),
                quote!(
                    struct Name(String);
                ),
                "duplicate identity capability",
            ),
            (
                quote!(custom(error = Error, admit = parse), fresh),
                quote!(
                    struct Name(String);
                ),
                "capability does not match",
            ),
            (
                quote!(text(Names), const_uuid),
                quote!(
                    struct Name(String);
                ),
                "capability does not match",
            ),
            (
                quote!(uuid(StoreIds), const_uuid, surreal = "invalid table"),
                quote!(
                    struct RowId(uuid::Uuid);
                ),
                "lowercase ASCII table",
            ),
            (
                quote!(text(Names)),
                quote!(
                    enum Name {
                        Value,
                    }
                ),
                "tuple struct",
            ),
            (
                quote!(text(Names)),
                quote!(
                    struct Pair(String, String);
                ),
                "one-field tuple",
            ),
            (
                quote!(text(Names)),
                quote!(
                    struct Generic<T>(T);
                ),
                "nongeneric",
            ),
            (
                quote!(text(Names)),
                quote!(
                    #[derive(Debug)]
                    struct Name(String);
                ),
                "owns standard derives",
            ),
            (
                quote!(text(Names)),
                quote!(
                    #[serde(transparent)]
                    struct Name(String);
                ),
                "owns Serde",
            ),
        ] {
            assert!(
                expand(arguments, item)
                    .unwrap_err()
                    .to_string()
                    .contains(message),
                "{message}"
            );
        }
    }
}
