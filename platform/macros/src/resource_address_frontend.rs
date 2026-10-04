//! Compact owner-selected address forms reuse the route admission backend.
use super::resource_address::declaration::{
    Declaration as AddressDeclaration, Options, read_custom_options,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Expr, Ident, LitStr, Path, Token,
    parse::{Parse, ParseStream},
};

struct Declaration {
    shape: Ident,
    profile: Option<Path>,
    options: Vec<syn::Meta>,
    custom: Option<TokenStream>,
}
impl Parse for Declaration {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let shape: Ident = input.parse()?;
        let body;
        syn::parenthesized!(body in input);
        if shape == "custom" {
            let custom = body.parse()?;
            if !input.is_empty() {
                return Err(input.error("custom accepts existing route mechanics only"));
            }
            return Ok(Self {
                shape,
                profile: None,
                options: Vec::new(),
                custom: Some(custom),
            });
        }
        if !["cached", "cached_checked", "components", "routes"]
            .iter()
            .any(|name| shape == name)
        {
            return Err(syn::Error::new_spanned(&shape, "unsupported address form"));
        }
        let profile = body.parse()?;
        if !body.is_empty() {
            return Err(body.error("address form accepts one owner profile"));
        }
        let mut options = Vec::new();
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            options.push(input.parse()?);
        }
        Ok(Self {
            shape,
            profile: Some(profile),
            options,
            custom: None,
        })
    }
}
fn retain_attributes(input: &mut DeriveInput, keep: impl Fn(&syn::Attribute) -> bool) {
    input.attrs.retain(&keep);
    match &mut input.data {
        Data::Struct(data) => {
            for field in &mut data.fields {
                field.attrs.retain(&keep);
            }
        }
        Data::Enum(data) => {
            for variant in &mut data.variants {
                variant.attrs.retain(&keep);
                for field in &mut variant.fields {
                    field.attrs.retain(&keep);
                }
            }
        }
        Data::Union(_) => {}
    }
}
fn remove_resource(input: &mut DeriveInput) {
    retain_attributes(input, |attr| !attr.path().is_ident("resource"));
}
fn key(meta: &syn::Meta) -> syn::Result<String> {
    meta.path()
        .get_ident()
        .map(ToString::to_string)
        .ok_or_else(|| syn::Error::new_spanned(meta, "address option requires one name"))
}
fn value(meta: &syn::Meta) -> syn::Result<Expr> {
    let syn::Meta::NameValue(pair) = meta else {
        return Err(syn::Error::new_spanned(
            meta,
            "address option requires = value",
        ));
    };
    Ok(pair.value.clone())
}
fn preset(meta: &syn::Meta) -> syn::Result<String> {
    let Expr::Path(path) = value(meta)? else {
        return Err(syn::Error::new_spanned(
            meta,
            "address preset requires a name",
        ));
    };
    path.path
        .get_ident()
        .map(ToString::to_string)
        .ok_or_else(|| syn::Error::new_spanned(meta, "address preset requires one name"))
}

fn bare(meta: &syn::Meta) -> syn::Result<()> {
    if !matches!(meta, syn::Meta::Path(_)) {
        return Err(syn::Error::new_spanned(meta, "address flag must be bare"));
    }
    Ok(())
}
fn remove_schema_attributes(input: &mut DeriveInput) {
    retain_attributes(input, |attr| !attr.path().is_ident("schemars"));
}

fn retain_schema_attributes(input: &mut DeriveInput) {
    retain_attributes(input, |attribute| {
        ["doc", "schemars", "serde", "cfg", "cfg_attr"]
            .iter()
            .any(|name| attribute.path().is_ident(name))
    });
}

pub fn expand(arguments: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let declaration = syn::parse2::<Declaration>(arguments)?;
    let mut input = syn::parse2::<DeriveInput>(item)?;
    if let Some(custom) = declaration.custom {
        let options = read_custom_options(&input.attrs, custom)?;
        let address = AddressDeclaration::new(&input, options, false)?;
        let backend = super::resource_address::generate(&address)?;
        remove_resource(&mut input);
        return Ok(quote!(#input #backend));
    }
    for attribute in &input.attrs {
        let conflict = if attribute.path().is_ident("derive") {
            super::derive_policy::conflict(attribute, &["ResourceAddress"])?.is_some()
        } else {
            attribute.path().is_ident("serde") || attribute.path().is_ident("resource")
        };
        if conflict {
            return Err(syn::Error::new_spanned(
                attribute,
                "address forms own standard derives, serialization and route options",
            ));
        }
    }
    let profile = declaration.profile.unwrap();
    let name = input.ident.clone();
    let shape = declaration.shape.to_string();
    let cached = shape.starts_with("cached");
    let mut traits = if shape == "cached_checked" {
        "ordered"
    } else {
        "cloned"
    }
    .to_string();
    let mut constructor = if shape == "cached_checked" {
        "checked"
    } else if shape == "routes" {
        "owner"
    } else if shape == "components" {
        "wrapped"
    } else {
        "owned"
    }
    .to_string();
    let mut schema = match shape.as_str() {
        "cached_checked" => "string",
        "cached" => "derived",
        "components" => "owner",
        _ => "none",
    }
    .to_string();
    let mut parse = "as_ref".to_string();
    let mut to_uri = if shape == "components" {
        "borrowed"
    } else {
        "none"
    }
    .to_string();
    let mut display = cached || shape == "components";
    let mut from_str = false;
    let mut resource_uri = false;
    let mut wire = shape != "routes";
    let mut route_options = Options::default();
    let mut seen = std::collections::BTreeSet::new();
    for meta in &declaration.options {
        let k = key(meta)?;
        if !seen.insert(k.clone()) {
            return Err(syn::Error::new_spanned(meta, "duplicate address option"));
        }
        if [
            "display",
            "no_display",
            "from_str",
            "resource_uri",
            "wire",
            "no_wire",
            "allow_empty_query",
        ]
        .contains(&k.as_str())
        {
            bare(meta)?;
        }
        if matches!(k.as_str(), "wire" | "no_wire")
            && seen.contains(if k == "wire" { "no_wire" } else { "wire" })
            || matches!(k.as_str(), "display" | "no_display")
                && seen.contains(if k == "display" {
                    "no_display"
                } else {
                    "display"
                })
        {
            return Err(syn::Error::new_spanned(meta, "opposing address flags"));
        }
        match k.as_str() {
            "traits" => traits = preset(meta)?,
            "constructor" => constructor = preset(meta)?,
            "schema" => schema = preset(meta)?,
            "parse" => parse = preset(meta)?,
            "to_uri" => to_uri = preset(meta)?,
            "display" => display = true,
            "no_display" => display = false,
            "from_str" => from_str = true,
            "resource_uri" => resource_uri = true,
            "no_wire" => wire = false,
            "wire" => wire = true,
            "template" => {
                route_options.template = {
                    let expression = value(meta)?;
                    Some(syn::parse2(quote!(#expression))?)
                }
            }
            "input" => route_options.input = Some(value(meta)?),
            "validate" => route_options.validate = Some(value(meta)?),
            "canonical" => {
                let expression = value(meta)?;
                route_options.canonical = syn::parse2::<syn::LitBool>(quote!(#expression))?.value;
                route_options.canonical_set = true;
            }
            "allow_empty_query" => route_options.allow_empty_query = true,
            _ => return Err(syn::Error::new_spanned(meta, "unknown address option")),
        }
    }
    let derived_traits = match traits.as_str() {
        "cloned" => quote!(Clone, Debug, PartialEq, Eq),
        "ordered" => quote!(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash),
        "copied" => quote!(Clone, Copy, Debug, PartialEq, Eq),
        "none" => quote!(),
        _ => {
            return Err(syn::Error::new_spanned(
                &declaration.shape,
                "unsupported address trait preset",
            ));
        }
    };
    if !["owned", "borrowed", "checked", "owner", "wrapped"].contains(&constructor.as_str())
        || !["as_ref", "borrowed", "none"].contains(&parse.as_str())
        || !["none", "borrowed", "copied"].contains(&to_uri.as_str())
        || !["none", "derived", "string", "owner"].contains(&schema.as_str())
    {
        return Err(syn::Error::new_spanned(
            &declaration.shape,
            "unsupported address convenience preset",
        ));
    }
    if shape == "routes" && !matches!(input.data, Data::Enum(_))
        || shape != "routes" && !matches!(input.data, Data::Struct(_))
    {
        return Err(syn::Error::new_spanned(
            &input,
            "address form and representation disagree",
        ));
    }
    let error: syn::Type = syn::parse_quote!(<#profile as ::veoveo_types::ResourceProfile>::Error);
    route_options.error = Some(error.clone());
    route_options.route_error = Some(syn::parse_quote!(Self::__address_route_error));
    route_options.wire = wire;
    let mut address = AddressDeclaration::new(&input, route_options, true)?;
    let mut cache = None;
    let mut parameters = Vec::new();
    let mut values = Vec::new();
    let mut accessors = Vec::new();
    if let Data::Struct(data) = &input.data {
        for (index, (field, opts)) in data
            .fields
            .iter()
            .zip(&mut address.routes[0].settings)
            .enumerate()
        {
            let member = field
                .ident
                .as_ref()
                .map(|id| quote!(#id))
                .unwrap_or_else(|| {
                    let index = syn::Index::from(index);
                    quote!(#index)
                });
            let argument = field
                .ident
                .clone()
                .unwrap_or_else(|| format_ident!("component_{index}"));
            let admit = opts.admit.as_ref();
            let clone_accessor = opts.clone_accessor;
            let owned_accessor = opts.owned_accessor;
            if clone_accessor && owned_accessor
                || opts.copy_accessor && (clone_accessor || owned_accessor)
            {
                return Err(syn::Error::new_spanned(
                    field,
                    "clone accessor conflicts with copied or owned accessor",
                ));
            }
            if (clone_accessor || owned_accessor) && opts.accessor.is_none() {
                return Err(syn::Error::new_spanned(
                    field,
                    "accessor convenience requires accessor = name",
                ));
            }
            if opts.cache
                && ((opts.argument.is_some()
                    || opts.admit.is_some()
                    || opts.clone_accessor
                    || opts.owned_accessor)
                    || opts.accessor.is_some())
            {
                return Err(syn::Error::new_spanned(
                    field,
                    "cache cannot declare component conveniences",
                ));
            }
            if admit.is_some() && constructor != "checked" {
                return Err(syn::Error::new_spanned(
                    field,
                    "component admission requires a checked constructor",
                ));
            }
            if opts.cache {
                let is_uri = matches!(&field.ty,syn::Type::Path(path) if path.path.segments.last().is_some_and(|segment|segment.ident=="ResourceUri"));
                let is_string =
                    matches!(&field.ty,syn::Type::Path(path) if path.path.is_ident("String"));
                if !is_uri && !is_string {
                    return Err(syn::Error::new_spanned(
                        field,
                        "cache requires String or ResourceUri",
                    ));
                }
                cache = Some((member, is_uri));
                continue;
            }
            let ty = field.ty.clone();
            let mode = opts
                .argument
                .as_deref()
                .unwrap_or(if constructor == "borrowed" {
                    "borrowed"
                } else {
                    "owned"
                });
            let (argument_type, mut argument_value) = match mode {
                "owned" => (quote!(#ty), quote!(#argument)),
                "borrowed" => (quote!(&#ty), quote!(#argument.clone())),
                "optional_borrowed" => {
                    let inner = super::resource_address::option_inner(&ty).ok_or_else(|| {
                        syn::Error::new_spanned(
                            &ty,
                            "optional borrowed argument requires Option<T>",
                        )
                    })?;
                    (quote!(Option<&#inner>), quote!(#argument.cloned()))
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        field,
                        "unsupported address argument preset",
                    ));
                }
            };
            if let Some(admit) = admit {
                argument_value = quote!((#admit)(#argument_value)?);
            }
            parameters.push(quote!(#argument:#argument_type));
            values.push(argument_value);
            if clone_accessor || owned_accessor {
                let accessor = opts.accessor.take().ok_or_else(|| {
                    syn::Error::new_spanned(&*field, "clone_accessor requires accessor")
                })?;
                let getter = if owned_accessor {
                    quote!(pub fn #accessor(self)->#ty { self.#member })
                } else {
                    quote!(pub fn #accessor(&self)->#ty { self.#member.clone() })
                };
                accessors.push(getter);
            }
            if opts.error.is_none() {
                opts.error = Some(syn::parse_quote!(Self::__address_component_error));
            }
        }
    }
    if let Data::Enum(_) = &input.data {
        for route in &mut address.routes {
            for options in &mut route.settings {
                if options.argument.is_some()
                    || options.admit.is_some()
                    || options.clone_accessor
                    || options.owned_accessor
                {
                    return Err(syn::Error::new_spanned(
                        &input,
                        "enum component conveniences stay owner-defined",
                    ));
                }
                if options.error.is_none() {
                    options.error = Some(syn::parse_quote!(Self::__address_component_error));
                }
            }
        }
    }
    if cached && cache.is_none() || !cached && cache.is_some() {
        return Err(syn::Error::new_spanned(
            &input,
            "cached representation and address form disagree",
        ));
    }
    if constructor == "wrapped"
        && (shape != "components"
            || address.options.validate.is_some()
            || address.options.input.is_some())
    {
        return Err(syn::Error::new_spanned(
            &input,
            "wrapped construction requires components without input or relationship validation",
        ));
    }
    if shape == "routes" && constructor != "owner" {
        return Err(syn::Error::new_spanned(
            &input,
            "enum constructors stay owner-defined",
        ));
    }
    if resource_uri && !cache.as_ref().is_some_and(|(_, is_uri)| *is_uri) {
        return Err(syn::Error::new_spanned(
            &input,
            "resource_uri requires a ResourceUri cache",
        ));
    }
    let wrapped = if let Data::Struct(data) = &input.data {
        super::resource_address::field_shape(&data.fields, &quote!(Self), values.iter().cloned())
    } else {
        quote!()
    };
    let backend = super::resource_address::generate(&address)?;
    remove_resource(&mut input);
    let mut schema_input = input.clone();
    retain_schema_attributes(&mut schema_input);
    remove_schema_attributes(&mut input);
    let serde = wire.then(|| quote!(#[serde(try_from="String",into="String")]));
    let serde_derives = wire.then(|| {
        let separator = (traits != "none").then(|| quote!(,));
        quote!(#separator ::serde::Serialize, ::serde::Deserialize)
    });
    let derive = if traits == "none" && !wire {
        quote!()
    } else {
        quote!(#[derive(#derived_traits #serde_derives)])
    };
    let constructor_fn = if constructor == "owner" {
        quote!()
    } else if constructor == "wrapped" {
        quote!(pub fn new(#(#parameters),*) -> Self { #wrapped })
    } else if constructor == "checked" {
        quote!(pub fn new(#(#parameters),*)->Result<Self,#error>{Self::resource_from_parts(#(#values),*)})
    } else {
        quote!(pub fn new(#(#parameters),*)->Self {Self::resource_from_parts(#(#values),*).unwrap_or_else(|_|panic!("typed address construction must produce admitted components"))})
    };
    let parse_fn = match parse.as_str() {
        "none" => quote!(),
        "borrowed" => {
            quote!(pub fn parse(value:&str)->Result<Self,#error> { Self::__address_parse(value) })
        }
        _ => {
            quote!(pub fn parse(value:impl AsRef<str>)->Result<Self,#error>{Self::__address_parse(value.as_ref())})
        }
    };
    let to_uri_fn = match to_uri.as_str() {
        "none" => quote!(),
        "copied" => quote!(
            pub fn to_uri(self) -> ::veoveo_types::ResourceUri {
                self.resource_components_uri()
                    .unwrap_or_else(|_| panic!("admitted address components"))
            }
        ),
        _ => quote!(
            pub fn to_uri(&self) -> ::veoveo_types::ResourceUri {
                self.resource_components_uri()
                    .unwrap_or_else(|_| panic!("admitted address components"))
            }
        ),
    };
    let cache_fn=cache.as_ref().map(|(member,is_uri)| {
        let text=if *is_uri {quote!(self.#member.as_str())}else{quote!(&self.#member)};
        let resource=(*is_uri&&resource_uri).then(||quote!(pub fn as_resource_uri(&self)->&::veoveo_types::ResourceUri { &self.#member }));
        quote!(pub fn as_str(&self)->&str {#text} #resource)
    });
    let display_impl=display.then(|| {
        let text=if cache.is_some(){quote!(formatter.write_str(self.as_str()))}else{quote!(self.resource_components_uri().map_err(|_|::std::fmt::Error)?.fmt(formatter))};
        quote!(impl ::std::fmt::Display for #name {fn fmt(&self,formatter:&mut ::std::fmt::Formatter<'_>)->::std::fmt::Result{#text}})
    });
    let from_str_impl=from_str.then(||quote!(impl ::std::str::FromStr for #name {type Err=#error;fn from_str(value:&str)->Result<Self,Self::Err>{Self::__address_parse(value)}}));
    let schema_impl = if schema == "none" {
        quote!()
    } else if schema == "owner" {
        quote!(const _: () = { assert!(<#profile as ::veoveo_types::ResourceProfile>::SCHEMA.is_some(), "schema = owner requires ResourceProfile::SCHEMA"); };
        impl ::schemars::JsonSchema for #name {
            fn schema_name()->::std::borrow::Cow<'static,str>{stringify!(#name).into()}
            fn inline_schema()->bool{<#profile as ::veoveo_types::ResourceProfile>::SCHEMA.expect("owner schema profile").inline}
            fn json_schema(generator:&mut ::schemars::SchemaGenerator)->::schemars::Schema{(<#profile as ::veoveo_types::ResourceProfile>::SCHEMA.expect("owner schema profile").schema)(stringify!(#name),generator)}
        })
    } else {
        let mut helper = schema_input;
        helper.ident = format_ident!("__AddressSchema");
        let helper_name = LitStr::new(&name.to_string(), name.span());
        let with = (schema == "string").then(|| quote!(#[schemars(with="String")]));
        quote!(const _:()={#[allow(dead_code)] #[derive(::schemars::JsonSchema)] #[schemars(rename=#helper_name)] #with #serde #helper
            impl ::schemars::JsonSchema for #name {
                fn schema_name()->::std::borrow::Cow<'static,str>{<__AddressSchema as ::schemars::JsonSchema>::schema_name()}
                fn schema_id()->::std::borrow::Cow<'static,str>{<__AddressSchema as ::schemars::JsonSchema>::schema_id()}
                fn inline_schema()->bool{<__AddressSchema as ::schemars::JsonSchema>::inline_schema()}
                fn json_schema(generator:&mut ::schemars::SchemaGenerator)->::schemars::Schema{<__AddressSchema as ::schemars::JsonSchema>::json_schema(generator)}
            }
        };)
    };
    Ok(quote!(#derive #serde #input #backend
        impl #name {
            #constructor_fn #parse_fn #cache_fn #to_uri_fn #(#accessors)*
            fn __address_parse(value:&str)->Result<Self,#error>{let uri=::veoveo_types::ResourceUri::new(value).map_err(|error|<#profile as ::veoveo_types::ResourceProfile>::uri_error(stringify!(#name),error))?;<Self as ::veoveo_types::ResourceAddress>::parse(&uri)}
            fn __address_component_error(error:#error)->#error{error}
            fn __address_route_error(error: ::veoveo_types::ResourceRouteError)->#error{(<#profile as ::veoveo_types::ResourceProfile>::PROFILE.route_error)(stringify!(#name),error)}
        } #display_impl #from_str_impl #schema_impl))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_uses_shared_owner_and_field_admission() {
        let arguments = quote!(custom(
            template = "example://item/{id}",
            error = OwnerError,
            route_error = map_error
        ));
        let item = quote!(
            struct Item(#[resource(variable = "id")] ItemId);
        );
        syn::parse2::<syn::File>(expand(arguments.clone(), item).unwrap()).unwrap();
        for item in [
            quote!(
                #[resource(template = "example://other/{id}")]
                struct Item(#[resource(variable = "id")] ItemId);
            ),
            quote!(
                struct Item(#[resource(variable = "id", variable = "id")] ItemId);
            ),
            quote!(
                struct Item(#[resource(variable = "id", unknown)] ItemId);
            ),
            quote!(
                struct Item(#[resource(variable = "id", clone_accessor)] ItemId);
            ),
        ] {
            assert!(expand(arguments.clone(), item).is_err());
        }
    }

    #[test]
    fn split_field_hooks_emit_one_convenience_accessor() {
        let output = expand(
            quote!(cached_checked(Uris), template = "example://item/{id}"),
            quote!(
                struct Item {
                    #[resource(cache)]
                    wire: String,
                    #[resource(accessor = id)]
                    #[resource(clone_accessor, admit = admit_id)]
                    id: ItemId,
                }
            ),
        )
        .unwrap();
        syn::parse2::<syn::File>(output.clone()).unwrap();
        assert_eq!(output.to_string().matches("pub fn id (").count(), 1);
        assert!(output.to_string().contains("admit_id"));
    }

    #[test]
    fn forms_preserve_component_presets_and_admit_foreign_derives() {
        for (arguments, item) in [
            (
                quote!(cached_checked(Uris), template = "example://item/{id}"),
                quote!(
                    struct Checked {
                        #[resource(cache)]
                        wire: String,
                        #[resource(accessor = id)]
                        id: ItemId,
                    }
                ),
            ),
            (
                quote!(
                    cached(Uris),
                    template = "example://item/{id}",
                    constructor = borrowed,
                    from_str,
                    resource_uri
                ),
                quote!(
                    struct Cached {
                        #[resource(cache)]
                        wire: ResourceUri,
                        #[resource(accessor = id, clone_accessor)]
                        id: ItemId,
                    }
                ),
            ),
            (
                quote!(cached(Uris), template = "example://index{?cursor}"),
                quote!(
                    struct Index {
                        #[resource(cache)]
                        wire: ResourceUri,
                        #[resource(argument = optional_borrowed, accessor = cursor)]
                        cursor: Option<ItemId>,
                    }
                ),
            ),
            (
                quote!(
                    components(Uris),
                    template = "example://item/{id}",
                    traits = copied,
                    to_uri = copied
                ),
                quote!(
                    struct Component(
                        #[resource(variable = "id", accessor = id, owned_accessor)] ItemId,
                    );
                ),
            ),
            (
                quote!(routes(Uris), traits = none, wire),
                quote!(
                    enum Minimal {
                        #[resource(template = "example://root")]
                        Root,
                    }
                ),
            ),
            (
                quote!(routes(Uris), wire, schema = owner),
                quote!(
                    #[derive(Default)]
                    enum Routes {
                        #[default]
                        #[resource(template = "example://root")]
                        Root,
                    }
                ),
            ),
        ] {
            let output = expand(arguments, item).unwrap();
            syn::parse2::<syn::File>(output).unwrap();
        }
    }

    #[test]
    fn generated_schema_owns_metadata_and_proves_owner_profile_is_present() {
        let output = expand(
            quote!(cached_checked(Uris), template = "example://item/{id}"),
            quote!(
                #[schemars(description = "fixture")]
                struct Address {
                    #[resource(cache)]
                    wire: String,
                    #[resource(accessor = id)]
                    id: ItemId,
                }
            ),
        )
        .unwrap();
        let file = syn::parse2::<syn::File>(output).unwrap();
        let owner = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Struct(item) if item.ident == "Address" => Some(item),
                _ => None,
            })
            .unwrap();
        assert!(
            !owner
                .attrs
                .iter()
                .any(|attribute| attribute.path().is_ident("schemars"))
        );
        let schema = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Const(item) if item.ident == "_" => Some(quote!(#item).to_string()),
                _ => None,
            })
            .unwrap();
        assert!(schema.contains("description = \"fixture\""));
        let output = expand(
            quote!(components(Uris), template = "example://item/{id}"),
            quote!(
                struct Item(#[resource(variable = "id")] ItemId);
            ),
        )
        .unwrap()
        .to_string();
        assert!(output.contains("const _ : ()"));
        assert!(output.contains("SCHEMA . is_some"));
    }

    #[test]
    fn malformed_and_conflicting_presets_fail_before_generated_rust() {
        let item = quote!(
            struct Address {
                #[resource(cache)]
                wire: String,
                #[resource(accessor = id)]
                id: ItemId,
            }
        );
        for arguments in [
            quote!(
                cached_checked(Uris),
                template = "example://item/{id}",
                no_wire = false
            ),
            quote!(
                cached_checked(Uris),
                template = "example://item/{id}",
                display = false
            ),
            quote!(
                cached_checked(Uris),
                template = "example://item/{id}",
                wire,
                no_wire
            ),
            quote!(
                cached_checked(Uris),
                template = "example://item/{id}",
                no_wire,
                wire
            ),
            quote!(
                cached_checked(Uris),
                template = "example://item/{id}",
                display,
                no_display
            ),
            quote!(
                cached_checked(Uris),
                template = "example://item/{id}",
                from_str = true
            ),
            quote!(
                cached_checked(Uris),
                template = "example://item/{id}",
                resource_uri = true
            ),
            quote!(
                cached_checked(Uris),
                template = "example://item/{id}",
                traits = unknown
            ),
        ] {
            assert!(expand(arguments, item.clone()).is_err());
        }
        assert!(
            expand(
                quote!(
                    components(Uris),
                    template = "example://item/{id}",
                    validate = check
                ),
                quote!(
                    struct Item(#[resource(variable = "id")] ItemId);
                )
            )
            .is_err()
        );
        assert!(
            expand(
                quote!(components(Uris), template = "example://item/{id}"),
                quote!(
                    struct Item(#[resource(variable = "id", admit = check)] ItemId);
                )
            )
            .is_err()
        );
        assert!(
            expand(
                quote!(
                    cached(Uris),
                    template = "example://item/{id}",
                    constructor = wrapped
                ),
                item.clone()
            )
            .is_err()
        );
        for item in [
            quote!(
                #[derive(Clone)]
                struct Address {
                    #[resource(cache)]
                    wire: String,
                    id: ItemId,
                }
            ),
            quote!(
                struct Address {
                    #[resource(cache = false)]
                    wire: String,
                    id: ItemId,
                }
            ),
            quote!(
                struct Address {
                    #[resource(cache)]
                    wire: String,
                    #[resource(argument = owned, argument = borrowed)]
                    id: ItemId,
                }
            ),
            quote!(
                struct Address {
                    #[resource(cache)]
                    wire: String,
                    #[resource(admit = check)]
                    #[resource(admit = check)]
                    id: ItemId,
                }
            ),
            quote!(
                struct Address {
                    #[resource(cache)]
                    wire: String,
                    #[resource(accessor = id, clone_accessor, copy_accessor)]
                    id: ItemId,
                }
            ),
            quote!(
                struct Address {
                    #[resource(cache)]
                    wire: String,
                    #[resource(accessor = id, owned_accessor, copy_accessor)]
                    id: ItemId,
                }
            ),
            quote!(
                struct Address {
                    #[resource(cache)]
                    wire: String,
                    #[resource(accessor = id, clone_accessor, owned_accessor)]
                    id: ItemId,
                }
            ),
            quote!(
                struct Address {
                    #[resource(cache)]
                    wire: String,
                    #[resource(accessor = id, owned_accessor = false)]
                    id: ItemId,
                }
            ),
            quote!(
                struct Address {
                    #[resource(cache)]
                    wire: String,
                    #[resource(clone_accessor)]
                    id: ItemId,
                }
            ),
        ] {
            assert!(
                expand(
                    quote!(cached_checked(Uris), template = "example://item/{id}"),
                    item
                )
                .is_err()
            );
        }
    }
}
