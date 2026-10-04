use proc_macro::TokenStream;
use quote::quote;
use std::collections::BTreeSet;
use syn::{Data, DeriveInput, Fields, LitStr, ext::IdentExt};

pub fn expand(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    generate(&input)
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

fn generate(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "Vocabulary does not support generics",
        ));
    }
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "Vocabulary requires an enum of unit variants",
        ));
    };
    let mut scope = false;
    let mut task_type = false;
    let mut surreal = false;
    for attr in &input.attrs {
        if attr.path().is_ident("vocabulary") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("scope") {
                    scope = true;
                } else if meta.path.is_ident("task_type") {
                    task_type = true;
                } else if meta.path.is_ident("surreal") {
                    surreal = true;
                } else {
                    return Err(meta.error("expected scope, task_type or surreal"));
                }
                Ok(())
            })?;
        }
    }
    if scope && task_type {
        return Err(syn::Error::new_spanned(
            input,
            "scope and task_type profiles cannot be combined",
        ));
    }
    if task_type && data.variants.is_empty() {
        return Err(syn::Error::new_spanned(
            input,
            "Task vocabulary must contain an operation",
        ));
    }
    let name = &input.ident;
    let name_text = name.unraw().to_string();
    let mut variants = Vec::new();
    let mut spellings = Vec::new();
    let mut variant_attrs = Vec::new();
    let mut seen = BTreeSet::new();
    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) || variant.discriminant.is_some() {
            return Err(syn::Error::new_spanned(
                variant,
                "Vocabulary requires unit variants without discriminants",
            ));
        }
        let mut spelling = None;
        for attr in &variant.attrs {
            if attr.path().is_ident("vocabulary") {
                attr.parse_nested_meta(|meta| {
                    if !meta.path.is_ident("rename") {
                        return Err(meta.error("expected rename = \"wire spelling\""));
                    }
                    if spelling.is_some() {
                        return Err(meta.error("duplicate rename"));
                    }
                    spelling = Some(meta.value()?.parse::<LitStr>()?);
                    Ok(())
                })?;
            }
        }
        let spelling = spelling.unwrap_or_else(|| {
            LitStr::new(
                &snake_case(&variant.ident.unraw().to_string()),
                variant.ident.span(),
            )
        });
        let value = spelling.value();
        if value.is_empty() || !seen.insert(value.clone()) {
            return Err(syn::Error::new_spanned(
                &spelling,
                "vocabulary spellings must be nonempty and distinct",
            ));
        }
        variants.push(&variant.ident);
        spellings.push(spelling);
        variant_attrs.push(
            variant
                .attrs
                .iter()
                .filter(|attr| attr.path().is_ident("doc") || attr.path().is_ident("schemars"))
                .collect::<Vec<_>>(),
        );
    }
    let attrs = input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc") || attr.path().is_ident("schemars"))
        .collect::<Vec<_>>();
    let schema_renamed = attrs.iter().any(|attr| {
        attr.path().is_ident("schemars")
            && attr
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )
                .is_ok_and(|args| args.iter().any(|meta| meta.path().is_ident("rename")))
    });
    let schema_name = (!schema_renamed).then(|| quote! { #[schemars(rename = #name_text)] });
    let scope_hook = scope.then(|| quote! {
        impl ::veoveo_types::ScopeDefinition for #name {
            fn name(self) -> &'static ::veoveo_types::ScopeName {
                match self { #(Self::#variants => {
                    static NAME: ::std::sync::LazyLock<::veoveo_types::ScopeName> = ::std::sync::LazyLock::new(||
                        ::veoveo_types::ScopeName::new(#spellings).expect("invalid declared scope"));
                    &NAME
                }),* }
            }
        }
        impl ::std::convert::TryFrom<&::veoveo_types::ScopeName> for #name {
            type Error = ::veoveo_types::IdentifierError;
            fn try_from(name: &::veoveo_types::ScopeName) -> Result<Self, Self::Error> {
                <Self as ::veoveo_types::Vocabulary>::from_wire(name.as_str()).ok_or_else(||
                    ::veoveo_types::IdentifierError::new(name.as_str(), concat!("unknown ", #name_text, " scope")))
            }
        }
        impl ::std::convert::From<#name> for ::veoveo_types::ScopeName {
            fn from(value: #name) -> Self { ::veoveo_types::ScopeDefinition::name(value).clone() }
        }
    });
    let task_hook = task_type.then(|| quote! {
        impl ::veoveo_types::TaskTypeDefinition for #name {
            const ALL: &'static [Self] = <Self as ::veoveo_types::Vocabulary>::ALL;
            fn name(self) -> ::veoveo_types::TaskTypeName {
                match self { #(Self::#variants => const { ::veoveo_types::TaskTypeName::from_static(#spellings) }),* }
            }
        }
    });
    let (parse, wire) = if scope {
        (
            quote! { Self::try_from(&::veoveo_types::ScopeName::new(value)?) },
            quote! {
                impl ::serde::Serialize for #name {
                    fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                        ::serde::Serialize::serialize(::veoveo_types::ScopeDefinition::name(*self), serializer)
                    }
                }
                impl<'de> ::serde::Deserialize<'de> for #name {
                    fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                        let name = <::veoveo_types::ScopeName as ::serde::Deserialize>::deserialize(deserializer)?;
                        Self::try_from(&name).map_err(::serde::de::Error::custom)
                    }
                }
                impl ::schemars::JsonSchema for #name {
                    fn schema_name() -> ::std::borrow::Cow<'static, str> { ::std::borrow::Cow::Borrowed(#name_text) }
                    fn schema_id() -> ::std::borrow::Cow<'static, str> { ::std::borrow::Cow::Borrowed(concat!(module_path!(), "::", #name_text)) }
                    fn json_schema(_: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema { ::veoveo_types::scope_vocabulary_schema::<Self>() }
                }
            },
        )
    } else {
        (
            quote! { <Self as ::veoveo_types::Vocabulary>::parse(value) },
            quote! {
                const _: () = {
                    #[derive(::serde::Serialize, ::serde::Deserialize, ::schemars::JsonSchema)]
                    #[serde(rename = #name_text)]
                    #schema_name
                    #(#attrs)*
                    enum Wire {
                        #(#(#variant_attrs)* #[serde(rename = #spellings)] #variants),*
                    }
                    impl ::serde::Serialize for #name {
                        fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                            let wire = match *self { #(Self::#variants => Wire::#variants),* };
                            ::serde::Serialize::serialize(&wire, serializer)
                        }
                    }
                    impl<'de> ::serde::Deserialize<'de> for #name {
                        fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                            Ok(match <Wire as ::serde::Deserialize>::deserialize(deserializer)? { #(Wire::#variants => Self::#variants),* })
                        }
                    }
                    impl ::schemars::JsonSchema for #name {
                        fn schema_name() -> ::std::borrow::Cow<'static, str> { <Wire as ::schemars::JsonSchema>::schema_name() }
                        fn schema_id() -> ::std::borrow::Cow<'static, str> { <Wire as ::schemars::JsonSchema>::schema_id() }
                        fn json_schema(generator: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema { <Wire as ::schemars::JsonSchema>::json_schema(generator) }
                        fn inline_schema() -> bool { <Wire as ::schemars::JsonSchema>::inline_schema() }
                    }
                };
            },
        )
    };
    let surreal_hook = surreal.then(|| quote! {
        const _: () = {
            use ::surrealdb::types as surrealdb_types;
            #[derive(::surrealdb::types::SurrealValue)]
            #[surreal(untagged)]
            enum DatabaseWire { #(#[surreal(value = #spellings)] #variants),* }
            impl ::surrealdb::types::SurrealValue for #name {
                fn kind_of() -> ::surrealdb::types::Kind { <DatabaseWire as ::surrealdb::types::SurrealValue>::kind_of() }
                fn is_value(value: &::surrealdb::types::Value) -> bool { <DatabaseWire as ::surrealdb::types::SurrealValue>::is_value(value) }
                fn into_value(self) -> ::surrealdb::types::Value {
                    let wire = match self { #(Self::#variants => DatabaseWire::#variants),* };
                    ::surrealdb::types::SurrealValue::into_value(wire)
                }
                fn from_value(value: ::surrealdb::types::Value) -> Result<Self, ::surrealdb::types::Error> {
                    Ok(match <DatabaseWire as ::surrealdb::types::SurrealValue>::from_value(value)? { #(DatabaseWire::#variants => Self::#variants),* })
                }
            }
        };
    });
    let validation = if scope {
        quote! { const _: () = { #(assert!(::veoveo_types::is_scope_token(#spellings), "declared scope must follow OAuth scope-token syntax");)* }; }
    } else if task_type {
        quote! { const _: () = { #(const _: ::veoveo_types::TaskTypeName = ::veoveo_types::TaskTypeName::from_static(#spellings);)* }; }
    } else {
        quote! {}
    };
    Ok(quote! {
        #validation
        impl ::veoveo_types::Vocabulary for #name {
            const ALL: &'static [Self] = &[#(Self::#variants),*];
            const NAME: &'static str = #name_text;
            const UNKNOWN_RULE: &'static str = concat!("unknown ", #name_text, " value");
            fn as_str(self) -> &'static str { match self { #(Self::#variants => #spellings),* } }
        }
        impl #name {
            pub const ALL: &'static [Self] = <Self as ::veoveo_types::Vocabulary>::ALL;
            pub fn as_str(&self) -> &'static str { <Self as ::veoveo_types::Vocabulary>::as_str(*self) }
        }
        impl ::std::fmt::Display for #name {
            fn fmt(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result { formatter.write_str(self.as_str()) }
        }
        impl ::std::str::FromStr for #name {
            type Err = ::veoveo_types::IdentifierError;
            fn from_str(value: &str) -> Result<Self, Self::Err> { #parse }
        }
        #scope_hook #task_hook #wire #surreal_hook
    })
}

fn snake_case(value: &str) -> String {
    let mut result = String::new();
    for (index, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() && index != 0 {
            result.push('_');
        }
        result.push(ch.to_ascii_lowercase());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn declaration(source: &str) -> syn::Result<proc_macro2::TokenStream> {
        generate(&syn::parse_str::<DeriveInput>(source)?)
    }
    #[test]
    fn rejects_ambiguous_spellings_and_shapes_before_expansion() {
        for source in [
            "enum Empty { #[vocabulary(rename = \"\")] Value }",
            "enum Duplicate { #[vocabulary(rename = \"same\")] First, #[vocabulary(rename = \"same\")] Second }",
            "enum Payload { Value(u8) }",
            "enum Generic<T> { Value(T) }",
            "enum Discriminant { Value = 1 }",
            "struct NotAnEnum;",
            "#[vocabulary(scope, task_type)] enum Mixed { Value }",
            "#[vocabulary(task_type)] enum NoTasks {}",
            "#[vocabulary(unknown)] enum Unknown { Value }",
        ] {
            assert!(declaration(source).is_err(), "{source}");
        }
    }
    #[test]
    fn scope_validation_calls_the_foundation_and_emits_no_database_dependency() {
        let expansion = declaration(
            "#[vocabulary(scope)] enum Scope { #[vocabulary(rename = \"owner:read\")] Read }",
        )
        .unwrap()
        .to_string();
        assert!(expansion.contains("is_scope_token"));
        assert!(expansion.contains("scope_vocabulary_schema"));
        assert!(!expansion.contains("surrealdb"));
    }
    #[test]
    fn snake_case_matches_serde_unit_variant_rules() {
        assert_eq!(snake_case("ApiActivity"), "api_activity");
        assert_eq!(snake_case("HTTPStatus"), "h_t_t_p_status");
    }
}
