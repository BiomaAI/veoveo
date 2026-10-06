//! Thin address derive. Component matching and encoding live in veoveo-types.
use quote::quote;
use syn::ext::IdentExt;
use syn::{Fields, Type};

pub(crate) mod declaration;
use declaration::{
    Accessor, Argument, Cache, Component, Declaration, FieldKind, Role, RouteDeclaration,
};

pub(super) struct Expansion {
    pub tokens: proc_macro2::TokenStream,
    pub parameters: Vec<proc_macro2::TokenStream>,
    pub values: Vec<proc_macro2::TokenStream>,
    pub cache: Option<(syn::Member, bool)>,
}

pub(super) fn generate(declaration: &Declaration<'_>) -> Expansion {
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
    let mut parameters = Vec::new();
    let mut values = Vec::new();
    let mut cache = None;
    for (index, parsed) in declaration.routes.iter().enumerate() {
        let route = &parsed.route;
        let template = route.template();
        let target = if let Some(variant) = parsed.variant {
            let constant = template_constant(variant);
            template_constants.push(quote!(pub const #constant: &'static str = #template;));
            quote!(Self::#variant)
        } else {
            let root = route.root();
            template_constants.push(quote!(pub const RESOURCE_ROOT: &'static str = #root;));
            template_constants.push(quote!(pub const RESOURCE_TEMPLATE: &'static str = #template;));
            quote!(Self)
        };
        let emitted = emit_route(parsed, target, index);
        pattern_cases.push(emitted.pattern);
        descriptors.push(route.descriptor());
        templates.push(template.clone());
        parse_cases.push(emitted.parse);
        build_cases.push(emitted.build);
        cached_wire = cached_wire.or(emitted.wire);
        if parsed.variant.is_none() {
            constructors = Some(emitted.constructors);
            parameters = emitted.parameters;
            values = emitted.values;
            cache = emitted.cache;
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
    let wire_body = cached_wire.unwrap_or_else(|| quote!(self.resource_components_uri()));
    let schema_inline = options.schema_inline;
    let schema = options.schema.as_ref().map(|function| quote! {
        impl ::schemars::JsonSchema for #name {
            fn schema_name() -> ::std::borrow::Cow<'static, str> { stringify!(#name).into() }
            fn inline_schema() -> bool { #schema_inline }
            fn json_schema(generator: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema { let schema = (#function)(generator); ::veoveo_types::naming::static_resource_schema(schema, None, module_path!(), stringify!(#name), generator) }
        }
    });
    let tokens = quote! {
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
    };
    Expansion {
        tokens,
        parameters,
        values,
        cache,
    }
}

struct RouteEmission {
    pattern: proc_macro2::TokenStream,
    parse: proc_macro2::TokenStream,
    build: proc_macro2::TokenStream,
    wire: Option<proc_macro2::TokenStream>,
    constructors: proc_macro2::TokenStream,
    parameters: Vec<proc_macro2::TokenStream>,
    values: Vec<proc_macro2::TokenStream>,
    cache: Option<(syn::Member, bool)>,
}

fn emit_route(
    parsed: &RouteDeclaration<'_>,
    target: proc_macro2::TokenStream,
    index: usize,
) -> RouteEmission {
    let options = &parsed.options;
    let error = options.error.as_ref().unwrap();
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
    let mut patterns = Vec::new();
    let mut builders = [BuildFragments::default(), BuildFragments::default()];
    let mut owned = Vec::new();
    let mut borrowed = Vec::new();
    let mut arguments = Vec::new();
    let mut accessors = Vec::new();
    let mut parameters = Vec::new();
    let mut values = Vec::new();
    let mut cache = None;
    let mut wire = None;
    for field in &parsed.plan {
        let binding = &field.binding;
        let member = &field.member;
        let name = &field.argument;
        let storage = &field.ty;
        match &field.kind {
            FieldKind::Cache(kind) => {
                let (value, cached) = match kind {
                    Cache::String => (
                        quote!(uri.to_string()),
                        quote!(::veoveo_types::ResourceUri::new(self.#member.clone()).map_err(|error| (#route_error)(::veoveo_types::ResourceRouteError::Uri(error)))),
                    ),
                    Cache::Uri => (quote!(uri.clone()), quote!(Ok(self.#member.clone()))),
                };
                parsing.push(quote! { let #binding = #value; });
                wire = Some(quote!({ #validate_cached #cached }));
                cache = Some((member.clone(), matches!(kind, Cache::Uri)));
            }
            FieldKind::Component(component) => {
                let Component {
                    variable,
                    codec,
                    error,
                    role,
                    value_type: ty,
                    accessor,
                    argument,
                    admit,
                } = component.as_ref();
                let map_error = error.as_ref().map(|function| quote! { (#function)(error) })
                    .unwrap_or_else(|| quote! { let _ = error; (#route_error)(::veoveo_types::ResourceRouteError::Field) });
                let codec = match role {
                    Role::Tail => quote!(<#codec as ::veoveo_types::ResourceTailCodec<#ty>>),
                    _ => quote!(<#codec as ::veoveo_types::ResourceFieldCodec<#ty>>),
                };
                patterns.push(quote!(#variable => #codec::encoded_pattern(context)));
                let source = match role {
                    Role::Query => {
                        quote!(captured.query(#variable).map(|text| #codec::parse(text).map_err(|error| { #map_error })).transpose()?)
                    }
                    Role::Tail => {
                        quote!(#codec::parse(captured.tail(#variable).expect("declared tail")).map_err(|error| { #map_error })?)
                    }
                    Role::Scalar => {
                        quote!(#codec::parse(captured.scalar(#variable).expect("declared scalar")).map_err(|error| { #map_error })?)
                    }
                };
                parsing.push(quote!(let #binding = #source;));
                let bound = if matches!(role, Role::Query) {
                    quote!(#binding.as_ref())
                } else {
                    quote!(#binding)
                };
                for (builder, value) in builders.iter_mut().zip([bound, quote!(#name)]) {
                    match role {
                        Role::Query => builder.queries.push(quote!(if let Some(value) = #value { query.push((#variable, #codec::text(value))); })),
                        Role::Tail => builder.path.push(quote!(::veoveo_types::RouteBinding::Tail { variable: #variable, segments: #codec::segments(#value) })),
                        Role::Scalar => builder.path.push(quote!(::veoveo_types::RouteBinding::Scalar { variable: #variable, value: #codec::text(#value) })),
                    }
                }
                owned.push(quote!(#name: #storage));
                if matches!(role, Role::Query) {
                    borrowed.push(quote!(#name: Option<&#ty>));
                    arguments.push(quote!(#name.as_ref()));
                } else {
                    borrowed.push(quote!(#name: &#storage));
                    arguments.push(quote!(&#name));
                }
                let (argument_type, mut value) = match argument {
                    Argument::Owned => (quote!(#storage), quote!(#name)),
                    Argument::Borrowed => (quote!(&#storage), quote!(#name.clone())),
                    Argument::Optional(inner) => (quote!(Option<&#inner>), quote!(#name.cloned())),
                };
                if let Some(admit) = admit {
                    value = quote!((#admit)(#value)?);
                }
                parameters.push(quote!(#name:#argument_type));
                values.push(value);
                if let Some((accessor, mode)) = accessor {
                    accessors.push(match mode {
                        Accessor::Copy => quote!(pub fn #accessor(&self) -> #storage { self.#member }),
                        Accessor::Clone => quote!(pub fn #accessor(&self) -> #storage { self.#member.clone() }),
                        Accessor::Owned => quote!(pub fn #accessor(self) -> #storage { self.#member }),
                        Accessor::Optional(inner) => quote!(pub fn #accessor(&self) -> Option<&#inner> { self.#member.as_ref() }),
                        Accessor::Borrowed => quote!(pub fn #accessor(&self) -> &#storage { &self.#member }),
                    });
                }
            }
        }
    }
    let construction = field_shape(
        parsed.fields,
        &target,
        parsed.plan.iter().map(|field| {
            let name = &field.binding;
            quote!(#name)
        }),
    );
    let shape = field_shape(
        parsed.fields,
        &target,
        parsed.plan.iter().map(|field| {
            let name = &field.binding;
            if matches!(field.kind, FieldKind::Cache(_)) {
                quote!(_)
            } else {
                quote!(#name)
            }
        }),
    );
    let capture = if patterns.is_empty() {
        quote!(Self::RESOURCE_ROUTES[#index].capture(&parts).map_err(|error| (#route_error)(error))?;)
    } else {
        quote!(let captured = Self::RESOURCE_ROUTES[#index].capture(&parts).map_err(|error| (#route_error)(error))?;)
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
    let [components, constructor] = builders;
    let build = components.finish(index, route_error);
    let constructor = constructor.finish(index, route_error);
    RouteEmission {
        pattern: quote!(Self::RESOURCE_ROUTES[#index].wire_pattern_with(spelling, |variable, context| { match variable { #(#patterns,)* _ => None } })?),
        parse,
        build: quote!(#shape => { #build }),
        wire,
        parameters,
        values,
        cache,
        constructors: quote! {
            /// Encode admitted typed components through the shared route descriptor.
            fn resource_build_uri(#(#borrowed),*) -> Result<::veoveo_types::ResourceUri, #error> { #constructor }
            /// Construct a checked owner value and initialize its declared wire cache.
            pub fn resource_from_parts(#(#owned),*) -> Result<Self, #error> {
                let wire = Self::resource_build_uri(#(#arguments),*)?;
                <Self as ::veoveo_types::ResourceAddress>::parse(&wire)
            }
            #(#accessors)*
        },
    }
}

#[derive(Default)]
struct BuildFragments {
    path: Vec<proc_macro2::TokenStream>,
    queries: Vec<proc_macro2::TokenStream>,
}
impl BuildFragments {
    fn finish(self, index: usize, route_error: &syn::Expr) -> proc_macro2::TokenStream {
        let Self { path, queries } = self;
        let mutability = (!queries.is_empty()).then(|| quote!(mut));
        quote! {
            let path = [#(#path),*];
            let #mutability query = ::std::vec::Vec::new();
            #(#queries)*
            Self::RESOURCE_ROUTES[#index].build(&path, &query).map_err(|error| (#route_error)(error))
        }
    }
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
#[cfg(test)]
mod tests;

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
