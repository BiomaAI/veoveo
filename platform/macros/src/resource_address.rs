//! Thin address derive. Component matching and encoding live in veoveo-types.
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::{Fields, Type};

pub(crate) mod declaration;
use declaration::{Declaration, FieldOptions, Options, Route};

pub(super) fn generate(declaration: &Declaration<'_>) -> syn::Result<proc_macro2::TokenStream> {
    let options = &declaration.options;
    let error = options.error.as_ref().unwrap();
    let route_error = options.route_error.as_ref().unwrap();
    let name = declaration.name;
    let mut descriptors = Vec::new();
    let mut pattern_cases = Vec::new();
    let mut templates = Vec::new();
    let mut template_constants = Vec::new();
    let mut parse_cases = Vec::new();
    let mut build_cases = Vec::new();
    let mut cached_wire = None;
    let mut constructors = None;
    for (index, parsed) in declaration.routes.iter().enumerate() {
        let route = &parsed.route;
        let settings = &parsed.settings;
        let template = route.template();
        let fields = parsed.fields;
        let target = if let Some(variant) = parsed.variant {
            let constant = template_constant(variant);
            template_constants.push(quote!(pub const #constant: &'static str = #template;));
            quote!(Self::#variant)
        } else {
            let root = route.root();
            template_constants.push(quote!(pub const RESOURCE_ROOT: &'static str = #root;));
            template_constants.push(quote!(pub const RESOURCE_TEMPLATE: &'static str = #template;));
            constructors = Some(struct_helpers(route, fields, settings, options)?);
            quote!(Self)
        };
        pattern_cases.push(pattern_case(route, fields, settings, index)?);
        descriptors.push(route.descriptor());
        templates.push(template.clone());
        let (parse, build, wire) =
            implementations(route, fields, settings, target, index, &parsed.options)?;
        parse_cases.push(parse);
        build_cases.push(build);
        cached_wire = cached_wire.or(wire);
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
    let wire_body = cached_wire.unwrap_or_else(|| quote!(self.resource_components_uri()));
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
                #wire_body
            }
        }
        #conversions
        #schema
    })
}

fn pattern_case(
    route: &Route,
    fields: &Fields,
    settings: &[FieldOptions],
    index: usize,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut cases = Vec::new();
    for (field, settings) in fields.iter().zip(settings) {
        if settings.cache {
            continue;
        }
        let variable = &settings.variable;
        let codec = settings.codec.as_ref().expect("admitted component codec");
        let ty = &field.ty;
        let fragment = if settings.tail {
            quote!(<#codec as ::veoveo_types::ResourceTailCodec<#ty>>::encoded_pattern(context))
        } else {
            let ty = if route.queries.iter().any(|query| query == variable) {
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
    settings: &[FieldOptions],
    target: proc_macro2::TokenStream,
    index: usize,
    options: &Options,
) -> syn::Result<(
    proc_macro2::TokenStream,
    proc_macro2::TokenStream,
    Option<proc_macro2::TokenStream>,
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
    let mut names = Vec::new();
    let mut cache = None;
    let mut uses_capture = false;
    for (position, (field, field_options)) in fields.iter().zip(settings).enumerate() {
        let binding = format_ident!(
            "__resource_field_{position}",
            span = proc_macro2::Span::mixed_site()
        );
        names.push(binding.clone());
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
        let variable = &field_options.variable;
        let codec = field_options
            .codec
            .as_ref()
            .expect("admitted component codec");
        let map_error = field_options.error.as_ref().map(|function| quote! { (#function)(error) })
            .unwrap_or_else(|| quote! { let _ = error; (#route_error)(::veoveo_types::ResourceRouteError::Field) });
        if route.queries.iter().any(|name| name == variable) {
            let inner = option_inner(&field.ty)
                .ok_or_else(|| syn::Error::new_spanned(field, "query field must be Option<T>"))?;
            parsing.push(quote! {
                let #binding = captured.query(#variable).map(|text| <#codec as ::veoveo_types::ResourceFieldCodec<#inner>>::parse(text)
                    .map_err(|error| { #map_error })).transpose()?;
            });
        } else if field_options.tail {
            let ty = &field.ty;
            parsing.push(quote! {
                let #binding = <#codec as ::veoveo_types::ResourceTailCodec<#ty>>::parse(captured.tail(#variable).expect("declared tail"))
                    .map_err(|error| { #map_error })?;
            });
        } else {
            let ty = &field.ty;
            parsing.push(quote! {
                let #binding = <#codec as ::veoveo_types::ResourceFieldCodec<#ty>>::parse(captured.scalar(#variable).expect("declared scalar"))
                    .map_err(|error| { #map_error })?;
            });
        }
    }
    let construction = field_shape(fields, &target, names.iter().map(|name| quote!(#name)));
    let pattern = field_shape(
        fields,
        &target,
        names.iter().zip(settings).map(|(name, settings)| {
            if settings.cache {
                quote!(_)
            } else {
                quote!(#name)
            }
        }),
    );
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
    let values = names
        .iter()
        .zip(settings)
        .map(|(name, settings)| {
            if route.queries.contains(&settings.variable) {
                quote!(#name.as_ref())
            } else {
                quote!(#name)
            }
        })
        .collect::<Vec<_>>();
    let build_body = build_components(route, fields, settings, &values, index, route_error)?;
    let build = quote!(#pattern => { #build_body });
    // Only a struct can own a private cache; component fields stay unbound here.
    let wire = cache.map(|(position, _, string)| {
        let member = fields.iter().nth(position).unwrap().ident.clone()
            .map(syn::Member::Named)
            .unwrap_or_else(|| syn::Member::Unnamed(syn::Index::from(position)));
        let value = if string {
            quote!(::veoveo_types::ResourceUri::new(self.#member.clone()).map_err(|error| (#route_error)(::veoveo_types::ResourceRouteError::Uri(error))))
        } else {
            quote!(Ok(self.#member.clone()))
        };
        quote!({ #validate_cached #value })
    });
    Ok((parse, build, wire))
}

/// Emit the owner's field shape for either values or pattern bindings.
pub(super) fn field_shape(
    fields: &Fields,
    target: &proc_macro2::TokenStream,
    values: impl IntoIterator<Item = proc_macro2::TokenStream>,
) -> proc_macro2::TokenStream {
    let values = values.into_iter();
    match fields {
        Fields::Named(named) => {
            let members = named
                .named
                .iter()
                .map(|field| field.ident.as_ref().unwrap());
            quote!(#target { #(#members: #values),* })
        }
        Fields::Unnamed(_) => quote!(#target(#(#values),*)),
        Fields::Unit => quote!(#target),
    }
}

fn build_components(
    route: &Route,
    fields: &Fields,
    settings: &[FieldOptions],
    values: &[proc_macro2::TokenStream],
    index: usize,
    route_error: &syn::Expr,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut path = Vec::new();
    let mut queries = Vec::new();
    for ((field, settings), value) in fields.iter().zip(settings).zip(values) {
        if settings.cache {
            continue;
        }
        let variable = &settings.variable;
        let codec = settings.codec.as_ref().expect("admitted component codec");
        let ty = &field.ty;
        if route.queries.contains(variable) {
            let inner = option_inner(ty)
                .ok_or_else(|| syn::Error::new_spanned(field, "query field must be Option<T>"))?;
            queries.push(quote!(if let Some(value) = #value { query.push((#variable, <#codec as ::veoveo_types::ResourceFieldCodec<#inner>>::text(value))); }));
        } else if settings.tail {
            path.push(quote!(::veoveo_types::RouteBinding::Tail { variable: #variable, segments: <#codec as ::veoveo_types::ResourceTailCodec<#ty>>::segments(#value) }));
        } else {
            path.push(quote!(::veoveo_types::RouteBinding::Scalar { variable: #variable, value: <#codec as ::veoveo_types::ResourceFieldCodec<#ty>>::text(#value) }));
        }
    }
    let query_mutability = (!queries.is_empty()).then(|| quote!(mut));
    Ok(quote! {
        let path = [#(#path),*];
        let #query_mutability query = ::std::vec::Vec::new();
        #(#queries)*
        Self::RESOURCE_ROUTES[#index].build(&path, &query).map_err(|error| (#route_error)(error))
    })
}

pub(super) fn option_inner(ty: &Type) -> Option<&Type> {
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
    settings: &[FieldOptions],
    options: &Options,
) -> syn::Result<proc_macro2::TokenStream> {
    let error = options.error.as_ref().unwrap();
    let route_error = options.route_error.as_ref().unwrap();
    let mut owned_parameters = Vec::new();
    let mut borrowed_parameters = Vec::new();
    let mut arguments = Vec::new();
    let mut values = Vec::new();
    let mut accessors = Vec::new();
    for (index, (field, settings)) in fields.iter().zip(settings).enumerate() {
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
        if settings.cache {
            values.push(quote!());
            continue;
        }
        owned_parameters.push(quote!(#name: #ty));
        let variable = &settings.variable;
        if route.queries.contains(variable) {
            let inner = option_inner(ty)
                .ok_or_else(|| syn::Error::new_spanned(field, "query field must be Option<T>"))?;
            borrowed_parameters.push(quote!(#name: Option<&#inner>));
            arguments.push(quote!(#name.as_ref()));
        } else {
            borrowed_parameters.push(quote!(#name: &#ty));
            arguments.push(quote!(&#name));
        }
        values.push(quote!(#name));
        if let Some(accessor) = &settings.accessor {
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
    let build = build_components(route, fields, settings, &values, 0, route_error)?;
    Ok(quote! {
        /// Encode admitted typed components through the shared route descriptor.
        fn resource_build_uri(#(#borrowed_parameters),*) -> Result<::veoveo_types::ResourceUri, #error> {
            #build
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
