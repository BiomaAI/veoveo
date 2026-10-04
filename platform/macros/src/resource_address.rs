//! Thin address derive. Component matching and encoding live in veoveo-types.
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::{Data, DeriveInput, Fields, Type};

pub(crate) mod declaration;
use declaration::{Options, Route, read_options};

pub(super) fn generate(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            input,
            "ResourceAddress requires a nongeneric owner",
        ));
    }
    let options = read_options(&input.attrs)?;
    let error = options
        .error
        .as_ref()
        .ok_or_else(|| syn::Error::new_spanned(input, "resource requires error = OwnerError"))?;
    let route_error = options.route_error.as_ref().ok_or_else(|| {
        syn::Error::new_spanned(input, "resource requires route_error = owner_mapping")
    })?;
    let name = &input.ident;
    let mut descriptors = Vec::new();
    let mut pattern_cases = Vec::new();
    let mut templates = Vec::new();
    let mut template_constants = Vec::new();
    let mut parse_cases = Vec::new();
    let mut build_cases = Vec::new();
    let mut wire_cases = Vec::new();
    let mut constructors = None;
    let mut routes: Vec<Route> = Vec::new();
    match &input.data {
        Data::Struct(data) => {
            let cached = data.fields.iter().enumerate().any(|(index, field)| {
                declaration::read_field(field, index).is_ok_and(|options| options.cache)
            });
            if cached
                && data
                    .fields
                    .iter()
                    .any(|field| !matches!(field.vis, syn::Visibility::Inherited))
            {
                return Err(syn::Error::new_spanned(
                    &data.fields,
                    "every field of a cached resource must be private to its owner module",
                ));
            }
            let route = Route::new(&options, &data.fields)?;
            pattern_cases.push(pattern_case(&route, &data.fields, 0)?);
            descriptors.push(route.descriptor());
            let template = route.template();
            let root = route.root();
            template_constants.push(quote!(pub const RESOURCE_ROOT: &'static str = #root;));
            templates.push(template.clone());
            template_constants.push(quote!(pub const RESOURCE_TEMPLATE: &'static str = #template;));
            constructors = Some(struct_helpers(&route, &data.fields, &options)?);
            let (parse, build, wire) =
                implementations(&route, &data.fields, quote!(Self), 0, &options)?;
            parse_cases.push(parse);
            build_cases.push(build);
            wire_cases.push(wire);
        }
        Data::Enum(data) => {
            if options.template.is_some() {
                return Err(syn::Error::new_spanned(
                    input,
                    "enum routes declare templates on each variant",
                ));
            }
            for (index, variant) in data.variants.iter().enumerate() {
                if variant.discriminant.is_some() {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "resource variants cannot have numeric discriminants",
                    ));
                }
                if variant.fields.iter().enumerate().any(|(index, field)| {
                    declaration::read_field(field, index).is_ok_and(|options| options.cache)
                }) {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "enum variants cannot hold externally private resource caches",
                    ));
                }
                let variant_options = declaration::read_variant_options(&variant.attrs, &options)?;
                if variant.fields.iter().enumerate().any(|(index, field)| {
                    declaration::read_field(field, index)
                        .is_ok_and(|options| options.accessor.is_some())
                }) {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "enum accessors stay owner-defined",
                    ));
                }
                let route = Route::new(&variant_options, &variant.fields)?;
                if routes.iter().any(|other| route.overlaps(other)) {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "resource route overlaps another variant's component shape",
                    ));
                }
                pattern_cases.push(pattern_case(&route, &variant.fields, index)?);
                descriptors.push(route.descriptor());
                let variant_name = &variant.ident;
                let template = route.template();
                templates.push(template.clone());
                let constant = template_constant(variant_name);
                template_constants.push(quote!(pub const #constant: &'static str = #template;));
                let (parse, build, wire) = implementations(
                    &route,
                    &variant.fields,
                    quote!(Self::#variant_name),
                    index,
                    &variant_options,
                )?;
                routes.push(route);
                parse_cases.push(parse);
                build_cases.push(build);
                wire_cases.push(wire);
            }
        }
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "ResourceAddress requires a struct or enum",
            ));
        }
    }
    let validate_self = options
        .validate
        .as_ref()
        .map(|function| quote! { (#function)(self)?; });
    let conversions = options.wire.then(|| quote! {
        impl ::std::convert::TryFrom<::std::string::String> for #name {
            type Error = #error;
            fn try_from(value: ::std::string::String) -> Result<Self, Self::Error> {
                let uri = ::veoveo_types::ResourceUri::new(value)
                    .map_err(|error| (#route_error)(::veoveo_types::ResourceRouteError::Uri(error)))?;
                <Self as ::veoveo_types::ResourceAddress>::parse(&uri)
            }
        }
        impl ::std::convert::From<#name> for ::std::string::String {
            fn from(value: #name) -> Self {
                <#name as ::veoveo_types::ResourceAddress>::to_uri(&value)
                    .expect("owner-admitted resource address").into()
            }
        }
    });
    let schema_inline = options.schema_inline;
    let schema = options.schema.as_ref().map(|function| quote! {
        impl ::schemars::JsonSchema for #name {
            fn schema_name() -> ::std::borrow::Cow<'static, str> { stringify!(#name).into() }
            fn inline_schema() -> bool { #schema_inline }
            fn json_schema(generator: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema { (#function)(generator) }
        }
    });
    Ok(quote! {
        impl #name {
            pub const RESOURCE_ROUTES: &'static [::veoveo_types::ResourceRoute] = &[#(#descriptors),*];
            pub const RESOURCE_TEMPLATES: &'static [&'static str] = &[#(#templates),*];
            #(#template_constants)*
            #constructors
            /// Owner codec fragments compose with shared encoded route patterns.
            pub fn resource_wire_patterns(spelling: ::veoveo_types::ResourcePatternSpelling)
                -> Result<::std::vec::Vec<::std::string::String>, ::veoveo_types::ResourceRouteError> {
                Ok(vec![#(#pattern_cases),*])
            }
            /// Build from typed fields; owners can use this when constructing a cached wire.
            pub fn resource_components_uri(&self) -> Result<::veoveo_types::ResourceUri, #error> {
                #validate_self
                match self { #(#build_cases),* }
            }
        }
        impl ::veoveo_types::ResourceAddress for #name {
            type Error = #error;
            fn parse(uri: &::veoveo_types::ResourceUri) -> Result<Self, Self::Error> {
                let parts = uri.components().map_err(|error| (#route_error)(::veoveo_types::ResourceRouteError::Uri(error)))?;
                #(#parse_cases)*
                Err((#route_error)(::veoveo_types::ResourceRouteError::Shape))
            }
            fn to_uri(&self) -> Result<::veoveo_types::ResourceUri, Self::Error> {
                match self { #(#wire_cases),* }
            }
        }
        #conversions
        #schema
    })
}

fn pattern_case(
    route: &Route,
    fields: &Fields,
    index: usize,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut cases = Vec::new();
    for (position, field) in fields.iter().enumerate() {
        let settings = declaration::read_field(field, position)?;
        if settings.cache {
            continue;
        }
        let variable = settings.variable;
        let codec = settings
            .codec
            .unwrap_or_else(|| syn::parse_quote!(::veoveo_types::IdentityResourceCodec));
        let ty = &field.ty;
        let fragment = if settings.tail {
            quote!(<#codec as ::veoveo_types::ResourceTailCodec<#ty>>::encoded_pattern(context))
        } else {
            let ty = if route.queries.iter().any(|query| query == &variable) {
                option_inner(ty).ok_or_else(|| {
                    syn::Error::new_spanned(field, "query field must be Option<T>")
                })?
            } else {
                ty
            };
            quote!(<#codec as ::veoveo_types::ResourceFieldCodec<#ty>>::encoded_pattern(context))
        };
        cases.push(quote!(#variable => #fragment));
    }
    Ok(
        quote!(Self::RESOURCE_ROUTES[#index].wire_pattern_with(spelling, |variable, context| {
        match variable { #(#cases,)* _ => None }
    })?),
    )
}

fn implementations(
    route: &Route,
    fields: &Fields,
    target: proc_macro2::TokenStream,
    index: usize,
    options: &Options,
) -> syn::Result<(
    proc_macro2::TokenStream,
    proc_macro2::TokenStream,
    proc_macro2::TokenStream,
)> {
    let route_error = options.route_error.as_ref().unwrap();
    let validate_cached = options
        .validate
        .as_ref()
        .map(|function| quote! { (#function)(self)?; });
    let canonical = options.canonical;
    let input_policy = options
        .input
        .as_ref()
        .map(|function| quote! { (#function)(&parts)?; });
    let mut parsing = Vec::new();
    let mut bindings = Vec::new();
    let mut queries = Vec::new();
    let mut names = Vec::new();
    let mut cache = None;
    let mut uses_capture = false;
    for (position, field) in fields.iter().enumerate() {
        let binding = format_ident!(
            "__resource_field_{position}",
            span = proc_macro2::Span::mixed_site()
        );
        names.push(binding.clone());
        let field_options = declaration::read_field(field, position)?;
        if field_options.cache {
            cache = Some((position, binding.clone(), is_string(&field.ty)));
            let value = if is_string(&field.ty) {
                quote!(uri.to_string())
            } else {
                quote!(uri.clone())
            };
            parsing.push(quote! { let #binding = #value; });
            continue;
        }
        uses_capture = true;
        let variable = field_options.variable;
        let codec = field_options
            .codec
            .unwrap_or_else(|| syn::parse_quote!(::veoveo_types::IdentityResourceCodec));
        let map_error = field_options.error.map(|function| quote! { (#function)(error) })
            .unwrap_or_else(|| quote! { let _ = error; (#route_error)(::veoveo_types::ResourceRouteError::Field) });
        if route.queries.iter().any(|name| name == &variable) {
            let inner = option_inner(&field.ty)
                .ok_or_else(|| syn::Error::new_spanned(field, "query field must be Option<T>"))?;
            parsing.push(quote! {
                let #binding = captured.query(#variable).map(|text| <#codec as ::veoveo_types::ResourceFieldCodec<#inner>>::parse(text)
                    .map_err(|error| { #map_error })).transpose()?;
            });
            queries.push(quote! {
                if let Some(value) = #binding.as_ref() { query.push((#variable, <#codec as ::veoveo_types::ResourceFieldCodec<#inner>>::text(value))); }
            });
        } else if field_options.tail {
            let ty = &field.ty;
            parsing.push(quote! {
                let #binding = <#codec as ::veoveo_types::ResourceTailCodec<#ty>>::parse(captured.tail(#variable).expect("declared tail"))
                    .map_err(|error| { #map_error })?;
            });
            bindings.push(quote! { ::veoveo_types::RouteBinding::Tail { variable: #variable, segments: <#codec as ::veoveo_types::ResourceTailCodec<#ty>>::segments(#binding) } });
        } else {
            let ty = &field.ty;
            parsing.push(quote! {
                let #binding = <#codec as ::veoveo_types::ResourceFieldCodec<#ty>>::parse(captured.scalar(#variable).expect("declared scalar"))
                    .map_err(|error| { #map_error })?;
            });
            bindings.push(quote! { ::veoveo_types::RouteBinding::Scalar { variable: #variable, value: <#codec as ::veoveo_types::ResourceFieldCodec<#ty>>::text(#binding) } });
        }
    }
    let construction = match fields {
        Fields::Named(fields) => {
            let members = fields
                .named
                .iter()
                .map(|field| field.ident.as_ref().unwrap());
            quote! { #target { #(#members: #names),* } }
        }
        Fields::Unnamed(_) => quote! { #target(#(#names),*) },
        Fields::Unit => quote! { #target },
    };
    let build_names = names.iter().enumerate().map(|(position, name)| {
        let member = fields.iter().nth(position).unwrap().ident.as_ref();
        if cache
            .as_ref()
            .is_some_and(|(cache_position, _, _)| position == *cache_position)
        {
            if matches!(fields, Fields::Named(_)) {
                quote!(#member: _)
            } else {
                quote!(_)
            }
        } else if matches!(fields, Fields::Named(_)) {
            quote!(#member: #name)
        } else {
            quote!(#name)
        }
    });
    let pattern = match fields {
        Fields::Named(_) => quote! { #target { #(#build_names),* } },
        Fields::Unnamed(_) => quote! { #target(#(#build_names),*) },
        Fields::Unit => quote! { #target },
    };
    let capture = if uses_capture {
        quote!(let captured = Self::RESOURCE_ROUTES[#index].capture(&parts).map_err(|error| (#route_error)(error))?;)
    } else {
        quote!(Self::RESOURCE_ROUTES[#index].capture(&parts).map_err(|error| (#route_error)(error))?;)
    };
    let parse = quote! {
        if Self::RESOURCE_ROUTES[#index].matches_shape(&parts) {
            #capture
            #input_policy
            #(#parsing)*
            let value = #construction;
            let built = value.resource_components_uri()?;
            if #canonical && built.as_str() != uri.as_str() {
                return Err((#route_error)(::veoveo_types::ResourceRouteError::Shape));
            }
            return Ok(value);
        }
    };
    let query_mutability = (!queries.is_empty()).then(|| quote!(mut));
    let build = quote! {
        #pattern => {
            let path = [#(#bindings),*];
            let #query_mutability query = ::std::vec::Vec::new();
            #(#queries)*
            Self::RESOURCE_ROUTES[#index].build(&path, &query).map_err(|error| (#route_error)(error))
        }
    };
    let wire = if let Some((position, name, string)) = cache {
        let names = names.iter().enumerate().map(|(index, field)| {
            if matches!(fields, Fields::Named(_)) {
                let member = fields.iter().nth(index).unwrap().ident.as_ref();
                if index == position {
                    quote!(#member: #field)
                } else {
                    quote!(#member: _)
                }
            } else if index == position {
                quote!(#field)
            } else {
                quote!(_)
            }
        });
        let pattern = match fields {
            Fields::Named(_) => quote!(#target { #(#names),* }),
            Fields::Unnamed(_) => quote!(#target(#(#names),*)),
            Fields::Unit => unreachable!(),
        };
        let value = if string {
            quote!(::veoveo_types::ResourceUri::new(#name.clone()).map_err(|error| (#route_error)(::veoveo_types::ResourceRouteError::Uri(error))))
        } else {
            quote!(Ok(#name.clone()))
        };
        quote!(#pattern => { #validate_cached #value })
    } else {
        let pattern = match fields {
            Fields::Named(_) => quote!(#target { .. }),
            Fields::Unnamed(_) => quote!(#target(..)),
            Fields::Unit => quote!(#target),
        };
        quote!(#pattern => self.resource_components_uri())
    };
    Ok((parse, build, wire))
}

fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    match arguments.args.first()? {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}
fn is_string(ty: &Type) -> bool {
    matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "String"))
}

#[cfg(test)]
mod tests;

fn struct_helpers(
    route: &Route,
    fields: &Fields,
    options: &Options,
) -> syn::Result<proc_macro2::TokenStream> {
    let error = options.error.as_ref().unwrap();
    let route_error = options.route_error.as_ref().unwrap();
    let mut owned_parameters = Vec::new();
    let mut borrowed_parameters = Vec::new();
    let mut arguments = Vec::new();
    let mut path = Vec::new();
    let mut queries = Vec::new();
    let mut accessors = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let name = field
            .ident
            .clone()
            .unwrap_or_else(|| format_ident!("field_{index}"));
        let member = field
            .ident
            .clone()
            .map(syn::Member::Named)
            .unwrap_or_else(|| syn::Member::Unnamed(syn::Index::from(index)));
        let ty = &field.ty;
        let settings = declaration::read_field(field, index)?;
        if settings.cache {
            continue;
        }
        owned_parameters.push(quote!(#name: #ty));
        let variable = settings.variable;
        let codec = settings
            .codec
            .unwrap_or_else(|| syn::parse_quote!(::veoveo_types::IdentityResourceCodec));
        if route.queries.contains(&variable) {
            let inner = option_inner(ty)
                .ok_or_else(|| syn::Error::new_spanned(field, "query field must be Option<T>"))?;
            borrowed_parameters.push(quote!(#name: Option<&#inner>));
            arguments.push(quote!(#name.as_ref()));
            queries.push(quote!(if let Some(value) = #name { query.push((#variable, <#codec as ::veoveo_types::ResourceFieldCodec<#inner>>::text(value))); }));
        } else {
            borrowed_parameters.push(quote!(#name: &#ty));
            arguments.push(quote!(&#name));
            if settings.tail {
                path.push(quote!(::veoveo_types::RouteBinding::Tail { variable: #variable, segments: <#codec as ::veoveo_types::ResourceTailCodec<#ty>>::segments(#name) }));
            } else {
                path.push(quote!(::veoveo_types::RouteBinding::Scalar { variable: #variable, value: <#codec as ::veoveo_types::ResourceFieldCodec<#ty>>::text(#name) }));
            }
        }
        if let Some(accessor) = settings.accessor {
            let method = if settings.copy_accessor {
                quote!(pub fn #accessor(&self) -> #ty { self.#member })
            } else if let Some(inner) = option_inner(ty) {
                quote!(pub fn #accessor(&self) -> Option<&#inner> { self.#member.as_ref() })
            } else {
                quote!(pub fn #accessor(&self) -> &#ty { &self.#member })
            };
            accessors.push(method);
        }
    }
    let query_mutability = (!queries.is_empty()).then(|| quote!(mut));
    Ok(quote! {
        /// Encode admitted typed components through the shared route descriptor.
        fn resource_build_uri(#(#borrowed_parameters),*) -> Result<::veoveo_types::ResourceUri, #error> {
            let path = [#(#path),*];
            let #query_mutability query = ::std::vec::Vec::new();
            #(#queries)*
            Self::RESOURCE_ROUTES[0].build(&path, &query).map_err(|error| (#route_error)(error))
        }
        /// Construct a checked owner value and initialize its declared wire cache.
        pub fn resource_from_parts(#(#owned_parameters),*) -> Result<Self, #error> {
            let wire = Self::resource_build_uri(#(#arguments),*)?;
            <Self as ::veoveo_types::ResourceAddress>::parse(&wire)
        }
        #(#accessors)*
    })
}

fn template_constant(variant: &syn::Ident) -> syn::Ident {
    let name = variant.unraw().to_string();
    let characters: Vec<_> = name.strip_prefix("r#").unwrap_or(&name).chars().collect();
    let mut spelling = String::from("RESOURCE_TEMPLATE_");
    for (index, character) in characters.iter().enumerate() {
        if index > 0
            && character.is_uppercase()
            && (characters[index - 1].is_lowercase()
                || characters[index - 1].is_numeric()
                || characters
                    .get(index + 1)
                    .is_some_and(|next| next.is_lowercase()))
        {
            spelling.push('_');
        }
        spelling.extend(character.to_uppercase());
    }
    syn::Ident::new(&spelling, variant.span())
}
