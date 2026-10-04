use quote::{ToTokens, quote};
use syn::{DeriveInput, Expr, Type};

/// One typed expansion configuration for ordinary profiles and explicit exceptions.
pub(super) struct Expansion {
    pub error: Type,
    pub admit: Option<Expr>,
    pub owned_admit: Option<Expr>,
    pub validate: Option<Expr>,
    pub text: Option<Expr>,
    pub string: bool,
    pub project: bool,
    pub wire_string: bool,
    pub generate: Option<Expr>,
    pub schema: Option<Expr>,
    pub schema_inline: bool,
    pub display: bool,
    pub inner_display: bool,
    pub extra: proc_macro2::TokenStream,
}
impl Expansion {
    pub fn custom(tokens: proc_macro2::TokenStream) -> syn::Result<Self> {
        use syn::parse::Parser;
        let mut config = Self {
            error: syn::parse_quote!(()),
            admit: None,
            owned_admit: None,
            validate: None,
            text: None,
            string: false,
            project: true,
            wire_string: false,
            generate: None,
            schema: None,
            schema_inline: false,
            display: true,
            inner_display: false,
            extra: proc_macro2::TokenStream::new(),
        };
        let mut has_error = false;
        let mut seen = std::collections::BTreeSet::new();
        syn::meta::parser(|meta| {
            if !seen.insert(meta.path.to_token_stream().to_string()) {
                return Err(meta.error("duplicate custom identity hook"));
            }
            if meta.path.is_ident("error") {
                config.error = meta.value()?.parse()?;
                has_error = true;
            } else if meta.path.is_ident("admit") {
                config.admit = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("validate") {
                config.validate = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("text") {
                config.text = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("generate") {
                config.generate = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("schema") {
                config.schema = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("string") {
                config.string = true;
            } else if meta.path.is_ident("wire_string") {
                config.wire_string = true;
            } else if meta.path.is_ident("schema_inline") {
                config.schema_inline = true;
            } else if meta.path.is_ident("no_display") {
                config.display = false;
            } else {
                return Err(meta.error("unknown custom identity hook"));
            }
            Ok(())
        })
        .parse2(tokens)?;
        if !has_error || config.admit.is_some() == config.validate.is_some() {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "custom identity requires error and exactly one admission hook",
            ));
        }
        if config.validate.is_some() && !config.string {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "validate requires String storage",
            ));
        }
        Ok(config)
    }
}
pub(super) fn emit(input: &DeriveInput, config: Expansion) -> proc_macro2::TokenStream {
    let Expansion {
        error,
        admit,
        owned_admit,
        validate,
        text,
        string,
        project,
        wire_string,
        generate,
        schema,
        schema_inline,
        display,
        inner_display,
        extra,
    } = config;
    let name = &input.ident;
    let custom_text = text.is_some();
    let text_hook = text.clone();
    let text = text
        .map(|text| quote! { (#text)(&self.0) })
        .unwrap_or_else(|| {
            if string {
                quote! { ::std::borrow::Cow::Borrowed(self.0.as_str()) }
            } else {
                quote! { ::std::borrow::Cow::Owned(self.0.to_string()) }
            }
        });
    let owned_admission = if let Some(admit) = &owned_admit {
        quote! { (#admit)(value).map(Self) }
    } else if let Some(validate) = &validate {
        quote! { (#validate)(&value)?; Ok(Self(value)) }
    } else {
        quote! { <Self as ::veoveo_types::Identity>::parse_identity(&value) }
    };
    let admission = if let Some(validate) = &validate {
        quote! { (#validate)(value)?; Ok(Self(value.to_owned())) }
    } else {
        let admit = admit.as_ref().unwrap();
        quote! { (#admit)(value).map(Self) }
    };
    let projection = (string && project).then(|| {
        quote! {
            impl #name {
                pub fn as_str(&self) -> &str { &self.0 }
            }
            impl ::std::convert::AsRef<str> for #name {
                fn as_ref(&self) -> &str { self.as_str() }
            }
        }
    });
    let string_impl = string.then(|| quote! {
        impl ::std::convert::TryFrom<::std::string::String> for #name {
            type Error = #error;
            fn try_from(value: ::std::string::String) -> Result<Self, Self::Error> { #owned_admission }
        }
        impl ::std::convert::From<#name> for ::std::string::String {
            fn from(value: #name) -> Self { value.0 }
        }
    });
    let wire_impl = wire_string.then(|| {
        quote! {
            impl ::std::convert::TryFrom<::std::string::String> for #name {
                type Error = #error;
                fn try_from(value: ::std::string::String) -> Result<Self, Self::Error> {
                    <Self as ::veoveo_types::Identity>::parse_identity(&value)
                }
            }
            impl ::std::convert::From<#name> for ::std::string::String {
                fn from(value: #name) -> Self {
                    <#name as ::veoveo_types::Identity>::identity_text(&value).into_owned()
                }
            }
        }
    });
    let generated_admission = if string {
        quote! { Self::try_from(inner).unwrap_or_else(|_| panic!("owner generator must produce an admitted identity")) }
    } else if let Some(text_hook) = text_hook {
        quote! {
            let text = (#text_hook)(&inner);
            <Self as ::veoveo_types::Identity>::parse_identity(text.as_ref())
                .unwrap_or_else(|_| panic!("owner generator must produce an admitted identity"))
        }
    } else if let Some(validate) = &validate {
        quote! {
            (#validate)(&inner)
                .unwrap_or_else(|_| panic!("owner generator must produce an admitted identity"));
            Self(inner)
        }
    } else {
        quote! {
            <Self as ::veoveo_types::Identity>::parse_identity(&inner.to_string())
                .unwrap_or_else(|_| panic!("owner generator must produce an admitted identity"))
        }
    };
    let generate_impl = generate.map(|generate| {
        quote! {
            impl #name {
                pub fn new() -> Self {
                    let inner = (#generate)();
                    #generated_admission
                }
            }
            impl ::std::default::Default for #name {
                fn default() -> Self { Self::new() }
            }
        }
    });
    let schema_impl = schema.map(|schema| {
        quote! {
            impl ::schemars::JsonSchema for #name {
                fn schema_name() -> ::std::borrow::Cow<'static, str> { stringify!(#name).into() }
                fn inline_schema() -> bool { #schema_inline }
                fn json_schema(generator: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema {
                    (#schema)(generator)
                }
            }
        }
    });
    let formatting = if custom_text {
        quote! { f.write_str(<Self as ::veoveo_types::Identity>::identity_text(self).as_ref()) }
    } else if string && !inner_display {
        quote! { f.write_str(&self.0) }
    } else {
        quote! { ::std::fmt::Display::fmt(&self.0, f) }
    };
    let display_impl = display.then(|| {
        quote! {
            impl ::std::fmt::Display for #name {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    #formatting
                }
            }
        }
    });
    quote! {
        impl ::veoveo_types::Identity for #name {
            type Error = #error;
            fn parse_identity(value: &str) -> Result<Self, Self::Error> {
                #admission
            }
            fn identity_text(&self) -> ::std::borrow::Cow<'_, str> { #text }
        }
        impl ::std::str::FromStr for #name {
            type Err = #error;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                <Self as ::veoveo_types::Identity>::parse_identity(value)
            }
        }
        #string_impl #projection
        impl #name {
            pub fn parse(value: impl AsRef<str>) -> Result<Self, #error> {
                <Self as ::veoveo_types::Identity>::parse_identity(value.as_ref())
            }
        }
        #wire_impl
        #generate_impl
        #schema_impl
        #display_impl
        #extra
    }
}
