use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Expr, Fields, Ident, Type};

pub fn expand(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    generate(&input)
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

fn generate(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            input,
            "Id requires a nongeneric newtype",
        ));
    }
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "Id requires a single-field tuple struct",
        ));
    };
    let Fields::Unnamed(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            input,
            "Id requires a single-field tuple struct",
        ));
    };
    if fields.unnamed.len() != 1 {
        return Err(syn::Error::new_spanned(
            input,
            "Id requires a single-field tuple struct",
        ));
    }
    let mut admit: Option<Expr> = None;
    let mut validate: Option<Expr> = None;
    let mut error: Option<Type> = None;
    let mut text: Option<Expr> = None;
    let mut string = false;
    let mut wire_string = false;
    let mut generate: Option<Expr> = None;
    let mut schema: Option<Expr> = None;
    let mut schema_inline = false;
    let mut display = true;
    let mut constructor: Option<Ident> = None;
    for attr in &input.attrs {
        if attr.path().is_ident("id") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("admit") {
                    if admit.is_some() {
                        return Err(meta.error("duplicate admit"));
                    }
                    admit = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("validate") {
                    if validate.is_some() {
                        return Err(meta.error("duplicate validate"));
                    }
                    validate = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("error") {
                    if error.is_some() {
                        return Err(meta.error("duplicate error"));
                    }
                    error = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("text") {
                    if text.is_some() {
                        return Err(meta.error("duplicate text"));
                    }
                    text = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("string") {
                    if string {
                        return Err(meta.error("duplicate string"));
                    }
                    string = true;
                } else if meta.path.is_ident("wire_string") {
                    wire_string = true;
                } else if meta.path.is_ident("generate") {
                    if generate.is_some() {
                        return Err(meta.error("duplicate generate"));
                    }
                    generate = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("schema") {
                    if schema.is_some() {
                        return Err(meta.error("duplicate schema"));
                    }
                    schema = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("schema_inline") {
                    schema_inline = true;
                } else if meta.path.is_ident("no_display") {
                    display = false;
                } else if meta.path.is_ident("constructor") {
                    if constructor.is_some() {
                        return Err(meta.error("duplicate constructor"));
                    }
                    constructor = Some(meta.value()?.parse()?);
                } else {
                    return Err(meta.error("unknown Id mechanics hook"));
                }
                Ok(())
            })?;
        }
    }
    if admit.is_some() == validate.is_some() {
        return Err(syn::Error::new_spanned(
            input,
            "Id requires exactly one owner admit or validate function",
        ));
    }
    if validate.is_some() && !string {
        return Err(syn::Error::new_spanned(
            input,
            "validate requires string mechanics",
        ));
    }
    let error =
        error.ok_or_else(|| syn::Error::new_spanned(input, "Id requires owner error = Type"))?;
    if string && wire_string {
        return Err(syn::Error::new_spanned(
            input,
            "string already includes wire_string conversions",
        ));
    }
    if string && generate.is_some() && constructor.is_none() {
        return Err(syn::Error::new_spanned(
            input,
            "generation requires a distinct parsing constructor",
        ));
    }
    if schema_inline && schema.is_none() {
        return Err(syn::Error::new_spanned(
            input,
            "schema_inline requires an owner schema hook",
        ));
    }
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
    let owned_admission = if let Some(validate) = &validate {
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
    let string_impl = string.then(|| {
        let constructor = constructor.clone().unwrap_or_else(|| Ident::new("new", name.span()));
        quote! {
            impl #name {
                pub fn #constructor(value: impl Into<::std::string::String>) -> Result<Self, #error> {
                    let value = value.into();
                    #owned_admission
                }
                pub fn as_str(&self) -> &str { &self.0 }
            }
            impl ::std::convert::AsRef<str> for #name {
                fn as_ref(&self) -> &str { self.as_str() }
            }
            impl ::std::convert::TryFrom<::std::string::String> for #name {
                type Error = #error;
                fn try_from(value: ::std::string::String) -> Result<Self, Self::Error> {
                    #owned_admission
                }
            }
            impl ::std::convert::From<#name> for ::std::string::String {
                fn from(value: #name) -> Self { value.0 }
            }
        }
    });
    let parse_impl = (!string)
        .then(|| {
            constructor.as_ref().map(|constructor| {
                quote! {
                    impl #name {
                        pub fn #constructor(value: impl AsRef<str>) -> Result<Self, #error> {
                            <Self as ::veoveo_types::Identity>::parse_identity(value.as_ref())
                        }
                    }
                }
            })
        })
        .flatten();
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
    let generated_admission = if let Some(text_hook) = text_hook {
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
    } else if string {
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
    Ok(quote! {
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
        #string_impl
        #parse_impl
        #wire_impl
        #generate_impl
        #schema_impl
        #display_impl
    })
}
