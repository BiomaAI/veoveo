use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::parse::Parser;
use syn::{Attribute, Data, DeriveInput, Expr, Field, Fields, LitBool, LitStr, Type};

#[derive(Clone)]
pub(crate) struct Options {
    pub template: Option<LitStr>,
    pub error: Option<Type>,
    pub route_error: Option<Expr>,
    pub validate: Option<Expr>,
    pub input: Option<Expr>,
    pub schema: Option<Expr>,
    pub schema_inline: bool,
    pub canonical: bool,
    pub canonical_set: bool,
    pub allow_empty_query: bool,
    pub wire: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            template: None,
            error: None,
            route_error: None,
            validate: None,
            input: None,
            schema: None,
            schema_inline: false,
            canonical: true,
            canonical_set: false,
            allow_empty_query: false,
            wire: false,
        }
    }
}
pub(crate) fn read_options(attrs: &[Attribute]) -> syn::Result<Options> {
    read_custom_options(attrs, proc_macro2::TokenStream::new())
}

pub(crate) fn read_custom_options(
    attrs: &[Attribute],
    tokens: proc_macro2::TokenStream,
) -> syn::Result<Options> {
    let mut options = Options::default();
    let mut seen = std::collections::BTreeSet::new();
    let mut parse = |meta: syn::meta::ParseNestedMeta<'_>| {
        if !seen.insert(meta.path.get_ident().map(ToString::to_string)) {
            return Err(meta.error("duplicate resource owner hook"));
        }
        if meta.path.is_ident("template") {
            options.template = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("error") {
            options.error = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("route_error") {
            options.route_error = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("validate") {
            options.validate = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("input") {
            options.input = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("schema") {
            options.schema = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("schema_inline") {
            options.schema_inline = true;
        } else if meta.path.is_ident("canonical") {
            options.canonical = meta.value()?.parse::<LitBool>()?.value;
            options.canonical_set = true;
        } else if meta.path.is_ident("allow_empty_query") {
            options.allow_empty_query = true;
        } else if meta.path.is_ident("wire") {
            options.wire = true;
        } else {
            return Err(meta.error("unknown resource owner hook"));
        }
        Ok(())
    };
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("resource")) {
        attr.parse_nested_meta(&mut parse)?;
    }
    syn::meta::parser(parse).parse2(tokens)?;
    if options.schema_inline && options.schema.is_none() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "schema_inline requires schema",
        ));
    }
    Ok(options)
}

pub(crate) fn read_variant_options(attrs: &[Attribute], parent: &Options) -> syn::Result<Options> {
    let variant = read_options(attrs)?;
    if variant.error.is_some()
        || variant.validate.is_some()
        || variant.input.is_some()
        || variant.schema.is_some()
        || variant.schema_inline
        || variant.wire
    {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "variant hooks are template, route_error, canonical and allow_empty_query; error, validate, input, schema and wire belong to the owner",
        ));
    }
    let mut options = parent.clone();
    options.template = variant.template;
    if variant.route_error.is_some() {
        options.route_error = variant.route_error;
    }
    if variant.canonical_set {
        options.canonical = variant.canonical;
    }
    options.allow_empty_query |= variant.allow_empty_query;
    Ok(options)
}

#[derive(Clone, Default)]
pub(crate) struct FieldOptions {
    pub variable: String,
    pub codec: Option<Type>,
    pub error: Option<Expr>,
    pub tail: bool,
    pub cache: bool,
    pub accessor: Option<syn::Ident>,
    pub copy_accessor: bool,
    pub argument: Option<String>,
    pub admit: Option<Expr>,
    pub clone_accessor: bool,
    pub owned_accessor: bool,
}
pub(crate) fn read_field(
    field: &Field,
    position: usize,
    conveniences: bool,
) -> syn::Result<FieldOptions> {
    let mut options = FieldOptions {
        variable: field
            .ident
            .as_ref()
            .map(|name| name.unraw().to_string())
            .unwrap_or_default(),
        ..FieldOptions::default()
    };
    let mut seen = std::collections::BTreeSet::new();
    for attr in field
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("resource"))
    {
        attr.parse_nested_meta(|meta| {
            let convenience = ["argument", "admit", "clone_accessor", "owned_accessor"]
                .iter()
                .any(|name| meta.path.is_ident(name));
            if !seen.insert(meta.path.get_ident().map(ToString::to_string)) {
                return Err(meta.error(if convenience && conveniences {
                    "duplicate address field convenience"
                } else {
                    "duplicate resource field hook"
                }));
            }
            if conveniences
                && [
                    "cache",
                    "tail",
                    "copy_accessor",
                    "clone_accessor",
                    "owned_accessor",
                ]
                .iter()
                .any(|name| meta.path.is_ident(name))
                && (meta.input.peek(syn::Token![=]) || meta.input.peek(syn::token::Paren))
            {
                return Err(meta.error("address flag must be bare"));
            }
            if meta.path.is_ident("variable") {
                options.variable = meta.value()?.parse::<LitStr>()?.value();
            } else if meta.path.is_ident("codec") {
                options.codec = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("error") {
                options.error = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("tail") {
                options.tail = true;
            } else if meta.path.is_ident("cache") {
                options.cache = true;
            } else if meta.path.is_ident("accessor") {
                options.accessor = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("copy_accessor") {
                options.copy_accessor = true;
            } else if conveniences && meta.path.is_ident("argument") {
                let name: syn::Path = meta.value()?.parse()?;
                options.argument = Some(
                    name.get_ident()
                        .ok_or_else(|| meta.error("address preset requires one name"))?
                        .to_string(),
                );
            } else if conveniences && meta.path.is_ident("admit") {
                options.admit = Some(meta.value()?.parse()?);
            } else if conveniences && meta.path.is_ident("clone_accessor") {
                options.clone_accessor = true;
            } else if conveniences && meta.path.is_ident("owned_accessor") {
                options.owned_accessor = true;
            } else {
                return Err(meta.error("unknown resource field hook"));
            }
            Ok(())
        })?;
    }
    if options.cache && !matches!(field.vis, syn::Visibility::Inherited) {
        return Err(syn::Error::new_spanned(
            field,
            "resource wire caches must be private to the owning module",
        ));
    }
    if options.cache && (options.tail || options.codec.is_some() || options.error.is_some()) {
        return Err(syn::Error::new_spanned(
            field,
            "cache cannot have a codec, tail or admission error hook",
        ));
    }
    if !options.cache && options.variable.is_empty() {
        return Err(syn::Error::new_spanned(
            field,
            format!("tuple field {position} requires an explicit variable"),
        ));
    }
    if options.copy_accessor && options.accessor.is_none() {
        return Err(syn::Error::new_spanned(
            field,
            "copy_accessor requires accessor = name",
        ));
    }
    if options.cache && options.accessor.is_some() {
        return Err(syn::Error::new_spanned(
            field,
            "cache access remains owner-defined",
        ));
    }
    if options.tail && options.codec.is_none() {
        return Err(syn::Error::new_spanned(
            field,
            "tail requires an explicit owner codec",
        ));
    }
    if !options.cache && options.codec.is_none() {
        options.codec = Some(syn::parse_quote!(::veoveo_types::IdentityResourceCodec));
    }
    Ok(options)
}

/// Admitted field roles are independent of optional Rust storage.
pub(crate) enum Role {
    Scalar,
    Query,
    Tail,
}
pub(crate) enum Cache {
    String,
    Uri,
}
pub(crate) enum Accessor {
    Borrowed,
    Optional(Box<Type>),
    Copy,
    Clone,
    Owned,
}
pub(crate) enum Argument {
    Owned,
    Borrowed,
    Optional(Box<Type>),
}
pub(crate) enum FieldKind {
    Cache(Cache),
    Component(Box<Component>),
}
pub(crate) struct Component {
    pub variable: String,
    pub codec: Type,
    pub error: Option<Expr>,
    pub role: Role,
    pub value_type: Type,
    pub accessor: Option<(syn::Ident, Accessor)>,
    pub argument: Argument,
    pub admit: Option<Expr>,
}
pub(crate) struct FieldPlan {
    pub member: syn::Member,
    pub binding: syn::Ident,
    pub argument: syn::Ident,
    pub ty: Type,
    pub kind: FieldKind,
}
impl FieldPlan {
    fn new(
        field: &Field,
        settings: FieldOptions,
        route: &Route,
        index: usize,
        constructor: Option<&str>,
        variant: bool,
    ) -> syn::Result<Self> {
        let FieldOptions {
            variable,
            codec,
            mut error,
            tail,
            cache,
            accessor,
            copy_accessor,
            argument,
            admit,
            clone_accessor,
            owned_accessor,
        } = settings;
        let invalid = |message| syn::Error::new_spanned(field, message);
        if constructor.is_some() {
            if variant
                && (argument.is_some() || admit.is_some() || clone_accessor || owned_accessor)
            {
                return Err(invalid("enum component conveniences stay owner-defined"));
            }
            if clone_accessor && owned_accessor
                || copy_accessor && (clone_accessor || owned_accessor)
            {
                return Err(invalid(
                    "clone accessor conflicts with copied or owned accessor",
                ));
            }
            if (clone_accessor || owned_accessor) && accessor.is_none() {
                return Err(invalid("accessor convenience requires accessor = name"));
            }
            if cache
                && (argument.is_some()
                    || admit.is_some()
                    || clone_accessor
                    || owned_accessor
                    || accessor.is_some())
            {
                return Err(invalid("cache cannot declare component conveniences"));
            }
            if admit.is_some() && constructor != Some("checked") {
                return Err(invalid(
                    "component admission requires a checked constructor",
                ));
            }
            if error.is_none() {
                error = Some(syn::parse_quote!(Self::__address_component_error));
            }
        }
        let optional = super::option_inner(&field.ty);
        let kind = if cache {
            // Custom hooks historically classify String by its final path segment;
            // compact forms require the unqualified String spelling.
            let string = matches!(&field.ty, Type::Path(path) if if constructor.is_some() { path.path.is_ident("String") } else { path.path.segments.last().is_some_and(|segment| segment.ident == "String") });
            if constructor.is_some()
                && !string
                && !matches!(&field.ty, Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "ResourceUri"))
            {
                return Err(invalid("cache requires String or ResourceUri"));
            }
            FieldKind::Cache(if string { Cache::String } else { Cache::Uri })
        } else {
            let role = if route.queries.contains(&variable) {
                Role::Query
            } else if tail {
                Role::Tail
            } else {
                Role::Scalar
            };
            let value_type = if matches!(role, Role::Query) {
                optional.ok_or_else(|| invalid("query field must be Option<T>"))?
            } else {
                &field.ty
            }
            .clone();
            let accessor = accessor.map(|name| {
                (
                    name,
                    if owned_accessor {
                        Accessor::Owned
                    } else if clone_accessor {
                        Accessor::Clone
                    } else if copy_accessor {
                        Accessor::Copy
                    } else if let Some(inner) = optional {
                        Accessor::Optional(Box::new(inner.clone()))
                    } else {
                        Accessor::Borrowed
                    },
                )
            });
            let argument = match argument
                .as_deref()
                .unwrap_or(if constructor == Some("borrowed") {
                    "borrowed"
                } else {
                    "owned"
                }) {
                "owned" => Argument::Owned,
                "borrowed" => Argument::Borrowed,
                "optional_borrowed" => Argument::Optional(Box::new(
                    optional
                        .ok_or_else(|| {
                            syn::Error::new_spanned(
                                &field.ty,
                                "optional borrowed argument requires Option<T>",
                            )
                        })?
                        .clone(),
                )),
                _ => return Err(invalid("unsupported address argument preset")),
            };
            FieldKind::Component(Box::new(Component {
                variable,
                codec: codec.expect("admitted component codec"),
                error,
                role,
                value_type,
                accessor,
                argument,
                admit,
            }))
        };
        Ok(Self {
            member: field
                .ident
                .clone()
                .map(syn::Member::Named)
                .unwrap_or_else(|| syn::Member::Unnamed(syn::Index::from(index))),
            binding: format_ident!(
                "__resource_field_{index}",
                span = proc_macro2::Span::mixed_site()
            ),
            argument: field
                .ident
                .clone()
                .unwrap_or_else(|| format_ident!("component_{index}")),
            ty: field.ty.clone(),
            kind,
        })
    }
}

pub(crate) struct Declaration<'a> {
    pub name: &'a syn::Ident,
    pub options: Options,
    pub routes: Vec<RouteDeclaration<'a>>,
}
pub(crate) struct RouteDeclaration<'a> {
    pub fields: &'a Fields,
    pub variant: Option<&'a syn::Ident>,
    pub options: Options,
    pub plan: Vec<FieldPlan>,
    pub route: Route,
}
impl<'a> Declaration<'a> {
    pub fn new(
        input: &'a DeriveInput,
        options: Options,
        constructor: Option<&str>,
    ) -> syn::Result<Self> {
        if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
            return Err(syn::Error::new_spanned(
                input,
                "ResourceAddress requires a nongeneric owner",
            ));
        }
        if options.error.is_none() {
            return Err(syn::Error::new_spanned(
                input,
                "resource requires error = OwnerError",
            ));
        }
        if options.route_error.is_none() {
            return Err(syn::Error::new_spanned(
                input,
                "resource requires route_error = owner_mapping",
            ));
        }
        let mut routes = Vec::new();
        let mut add = |fields: &'a Fields,
                       variant: Option<&'a syn::Variant>,
                       options: Options|
         -> syn::Result<()> {
            let settings = fields
                .iter()
                .enumerate()
                .map(|(index, field)| read_field(field, index, constructor.is_some()))
                .collect::<syn::Result<Vec<_>>>()?;
            if let Some(variant) = variant {
                if settings.iter().any(|options| options.cache) {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "enum variants cannot hold externally private resource caches",
                    ));
                }
                if settings.iter().any(|options| options.accessor.is_some()) {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "enum accessors stay owner-defined",
                    ));
                }
            } else if settings.iter().any(|options| options.cache)
                && fields
                    .iter()
                    .any(|field| !matches!(field.vis, syn::Visibility::Inherited))
            {
                return Err(syn::Error::new_spanned(
                    fields,
                    "every field of a cached resource must be private to its owner module",
                ));
            }
            let route = Route::new(&options, fields, &settings)?;
            if let Some(variant) = variant
                && routes
                    .iter()
                    .any(|(_, _, _, _, other)| route.overlaps(other))
            {
                return Err(syn::Error::new_spanned(
                    variant,
                    "resource route overlaps another variant's component shape",
                ));
            }
            routes.push((
                fields,
                variant.map(|variant| &variant.ident),
                options,
                settings,
                route,
            ));
            Ok(())
        };
        match &input.data {
            Data::Struct(data) => add(&data.fields, None, options.clone())?,
            Data::Enum(data) => {
                if options.template.is_some() {
                    return Err(syn::Error::new_spanned(
                        input,
                        "enum routes declare templates on each variant",
                    ));
                }
                for variant in &data.variants {
                    if variant.discriminant.is_some() {
                        return Err(syn::Error::new_spanned(
                            variant,
                            "resource variants cannot have numeric discriminants",
                        ));
                    }
                    add(
                        &variant.fields,
                        Some(variant),
                        read_variant_options(&variant.attrs, &options)?,
                    )?;
                }
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    input,
                    "ResourceAddress requires a struct or enum",
                ));
            }
        }
        Ok(Self {
            name: &input.ident,
            options,
            routes: routes
                .into_iter()
                .map(|(fields, variant, options, settings, route)| {
                    let plan = fields
                        .iter()
                        .zip(settings)
                        .enumerate()
                        .map(|(index, (field, settings))| {
                            FieldPlan::new(
                                field,
                                settings,
                                &route,
                                index,
                                constructor,
                                variant.is_some(),
                            )
                        })
                        .collect::<syn::Result<_>>()?;
                    Ok(RouteDeclaration {
                        fields,
                        variant,
                        options,
                        plan,
                        route,
                    })
                })
                .collect::<syn::Result<_>>()?,
        })
    }
}

pub(crate) enum Part {
    Literal(String),
    Scalar(String),
    Tail(String),
}
pub(crate) struct Route {
    template: LitStr,
    root: String,
    scheme: String,
    authority: String,
    path: Vec<Part>,
    pub queries: Vec<String>,
    trailing_slash: bool,
    allow_empty_query: bool,
    canonical_query_order: bool,
}
impl Route {
    pub fn new(options: &Options, fields: &Fields, settings: &[FieldOptions]) -> syn::Result<Self> {
        let template = options.template.clone().ok_or_else(|| {
            syn::Error::new_spanned(fields, "resource route requires template = literal")
        })?;
        let value = template.value();
        let invalid = || {
            syn::Error::new_spanned(
                &template,
                "expected literal scheme/authority, complete path variables, and an optional final {?query,...} expression",
            )
        };
        let (path_text, queries) = if let Some((path, query)) = value.split_once("{?") {
            let query = query.strip_suffix('}').ok_or_else(invalid)?;
            if query.is_empty() || query.contains(['{', '}']) {
                return Err(invalid());
            }
            (
                path,
                query.split(',').map(str::to_owned).collect::<Vec<_>>(),
            )
        } else {
            (value.as_str(), Vec::new())
        };
        let (scheme, rest) = path_text.split_once("://").ok_or_else(invalid)?;
        if scheme.is_empty()
            || !scheme.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_lowercase()
                    || (index > 0 && (byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.')))
            })
        {
            return Err(invalid());
        }
        let (authority, path) = rest
            .split_once('/')
            .map_or((rest, None), |(authority, path)| (authority, Some(path)));
        if authority.is_empty()
            || !authority
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(invalid());
        }
        let trailing_slash = path.is_some_and(|path| path.is_empty() || path.ends_with('/'));
        let segments = path
            .map(|path| {
                if trailing_slash {
                    path.strip_suffix('/').unwrap_or(path)
                } else {
                    path
                }
            })
            .unwrap_or_default();
        let mut parts = Vec::new();
        if !segments.is_empty() {
            for segment in segments.split('/') {
                if let Some(variable) = segment
                    .strip_prefix('{')
                    .and_then(|segment| segment.strip_suffix('}'))
                {
                    if let Some(variable) = variable.strip_prefix('+') {
                        parts.push(Part::Tail(variable.into()));
                    } else {
                        parts.push(Part::Scalar(variable.into()));
                    }
                } else {
                    if segment.is_empty()
                        || segment.contains(['{', '}', '?', '#', '%'])
                        || !segment.is_ascii()
                    {
                        return Err(invalid());
                    }
                    parts.push(Part::Literal(segment.into()));
                }
            }
        }
        let mut variables = std::collections::BTreeSet::new();
        for (index, part) in parts.iter().enumerate() {
            if let Part::Scalar(variable) | Part::Tail(variable) = part
                && (!valid_variable(variable)
                    || !variables.insert(variable.clone())
                    || (matches!(part, Part::Tail(_)) && index + 1 != parts.len()))
            {
                return Err(invalid());
            }
        }
        for variable in &queries {
            if !valid_variable(variable) || !variables.insert(variable.clone()) {
                return Err(invalid());
            }
        }
        let mut mapped = std::collections::BTreeSet::new();
        let mut cache_count = 0;
        for (field, options) in fields.iter().zip(settings) {
            if options.cache {
                cache_count += 1;
                continue;
            }
            if !variables.contains(&options.variable) || !mapped.insert(options.variable.clone()) {
                return Err(syn::Error::new_spanned(
                    field,
                    "field must map to exactly one declared template variable",
                ));
            }
            let tail = parts
                .iter()
                .any(|part| matches!(part, Part::Tail(variable) if variable == &options.variable));
            if tail != options.tail {
                return Err(syn::Error::new_spanned(
                    field,
                    "{+variable} requires a tail field and owner tail codec",
                ));
            }
        }
        if mapped != variables || cache_count > 1 {
            return Err(syn::Error::new_spanned(
                fields,
                "every template variable needs one field; at most one cache field is permitted",
            ));
        }
        Ok(Self {
            template,
            root: format!("{scheme}://{authority}"),
            scheme: scheme.into(),
            authority: authority.into(),
            path: parts,
            queries,
            trailing_slash,
            allow_empty_query: options.allow_empty_query,
            canonical_query_order: options.canonical,
        })
    }
    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn template(&self) -> &LitStr {
        &self.template
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        if self.scheme != other.scheme || self.authority != other.authority {
            return false;
        }
        fn shape(route: &Route) -> Vec<Option<&str>> {
            let mut shape: Vec<_> = route
                .path
                .iter()
                .map(|part| match part {
                    Part::Literal(value) => Some(value.as_str()),
                    Part::Scalar(_) | Part::Tail(_) => None,
                })
                .collect();
            if route.trailing_slash {
                shape.push(Some(""));
            }
            shape
        }
        let left = shape(self);
        let right = shape(other);
        let left_tail = self
            .path
            .iter()
            .position(|part| matches!(part, Part::Tail(_)));
        let right_tail = other
            .path
            .iter()
            .position(|part| matches!(part, Part::Tail(_)));
        let fixed = left_tail.into_iter().chain(right_tail).min();
        if let Some(prefix) = fixed {
            if left.len() < prefix || right.len() < prefix {
                return false;
            }
            return left
                .iter()
                .zip(&right)
                .take(prefix)
                .all(|(left, right)| left.is_none() || right.is_none() || left == right);
        }
        left.len() == right.len()
            && left
                .iter()
                .zip(&right)
                .all(|(left, right)| left.is_none() || right.is_none() || left == right)
    }

    pub fn descriptor(&self) -> proc_macro2::TokenStream {
        let Self {
            template,
            root,
            scheme,
            authority,
            path,
            queries,
            trailing_slash,
            allow_empty_query,
            canonical_query_order,
        } = self;
        let path = path.iter().map(|part| match part {
            Part::Literal(value) => quote!(::veoveo_types::RoutePath::Literal(#value)),
            Part::Scalar(value) => quote!(::veoveo_types::RoutePath::Scalar(#value)),
            Part::Tail(value) => quote!(::veoveo_types::RoutePath::Tail(#value)),
        });
        let query = queries
            .iter()
            .map(|value| quote!(::veoveo_types::RouteQuery { name: #value, variable: #value }));
        quote! {
            ::veoveo_types::ResourceRoute::declare(
                #template, #root, #scheme, #authority, &[#(#path),*], &[#(#query),*],
                ::veoveo_types::ResourceRoutePolicy {
                    allow_empty_query: #allow_empty_query, trailing_slash: #trailing_slash,
                    canonical_query_order: #canonical_query_order,
                },
            )
        }
    }
}
fn valid_variable(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic() || byte == b'_' || (index > 0 && byte.is_ascii_digit())
        })
}
