//! Component routes shared by owner parsers, builders and discovery declarations.
//! Owners supply field admission, relationships, errors and wire/schema policy.
use std::borrow::Cow;

use crate::{
    Identity, ResourceUri, ResourceUriBuilder, ResourceUriError, ResourceUriParts, UriSegment,
};

mod pattern;

/// An independent owner may provide a codec for an ID, version or opaque cursor.
/// This trait does not change the cursor's payload or serialization format.
/// The default codec requires an admitted identity, not a raw String:
/// ```compile_fail
/// use veoveo_types::{IdentityResourceCodec, ResourceFieldCodec};
/// let _ = <IdentityResourceCodec as ResourceFieldCodec<String>>::parse("unchecked");
/// ```
pub trait ResourceFieldCodec<T> {
    type Error;
    fn parse(value: &str) -> Result<T, Self::Error>;
    fn text(value: &T) -> Cow<'_, str>;
    /// An owner-supplied regex fragment for encoded wire text, without anchors.
    /// Return None when the requested encoding or spelling cannot be described.
    /// The fragment declares empty admission separately from its regex.
    fn encoded_pattern(_context: ResourcePatternContext) -> Option<ResourceEncodedPattern> {
        None
    }
}

/// Identity admission is an explicit codec choice; raw String has no implementation.
pub struct IdentityResourceCodec;
impl<T: Identity> ResourceFieldCodec<T> for IdentityResourceCodec {
    type Error = T::Error;
    fn parse(value: &str) -> Result<T, Self::Error> {
        T::parse_identity(value)
    }
    fn text(value: &T) -> Cow<'_, str> {
        value.identity_text()
    }
}

/// A domain value occupying multiple already-decoded path segments.
/// The owner determines whether an escaped slash inside one segment is admissible.
pub trait ResourceTailCodec<T> {
    type Error;
    fn parse(segments: &[Cow<'_, str>]) -> Result<T, Self::Error>;
    fn segments(value: &T) -> Vec<Cow<'_, str>>;
    /// Describes the entire encoded tail, excluding its preceding slash.
    fn encoded_pattern(_context: ResourcePatternContext) -> Option<ResourceEncodedPattern> {
        None
    }
}

/// A grouped regex fragment for encoded text. Empty admission is explicit because
/// a bare query name and an absent tail contain no text to match against the regex.
#[derive(Debug, Clone)]
pub struct ResourceEncodedPattern {
    pub pattern: Cow<'static, str>,
    pub allows_empty: bool,
}

/// Component encoding selected by the shared URI builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceComponentEncoding {
    PathSegment,
    PathTail,
    QueryValue,
}

/// Canonical describes builder output; Admitted includes owner-supported aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourcePatternSpelling {
    Canonical,
    Admitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourcePatternContext {
    pub encoding: ResourceComponentEncoding,
    pub spelling: ResourcePatternSpelling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutePath {
    Literal(&'static str),
    Scalar(&'static str),
    Tail(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteQuery {
    pub name: &'static str,
    pub variable: &'static str,
}

/// A literal declaration. A tail may occur only at the end of the path.
/// Optional query declarations preserve their array order when building.
/// The attribute requires owner fields to map to template variables and keeps
/// constructors in the owning library. Unmapped fields cannot become components.
/// ```compile_fail
/// #[veoveo_types::resource_address(custom(template = "example://items/{id}", error = veoveo_types::ResourceUriError,
///     route_error = |_| veoveo_types::ResourceUriError::InvalidUri))]
/// struct Unmapped { other: veoveo_types::PrincipalId }
/// ```
/// Existing custom field codecs compose with the attribute's checked backend:
/// ```
/// #[veoveo_types::resource_address(custom(
///     template = "example://items/{id}", error = veoveo_types::ResourceUriError,
///     route_error = |_| veoveo_types::ResourceUriError::InvalidUri))]
/// struct Address { id: veoveo_types::PrincipalId }
/// let id = veoveo_types::PrincipalId::parse("actor").unwrap();
/// let address = Address::resource_from_parts(id).unwrap();
/// let uri = veoveo_types::ResourceAddress::to_uri(&address).unwrap();
/// assert_eq!(uri.as_str(), "example://items/actor");
/// ```
/// A raw String cannot satisfy the default field codec.
/// ```compile_fail
/// #[veoveo_types::resource_address(custom(template = "example://items/{id}", error = veoveo_types::ResourceUriError,
///     route_error = |_| veoveo_types::ResourceUriError::InvalidUri))]
/// struct Unchecked { id: String }
/// ```
/// Generic owners cannot declare the attribute's static route vocabulary.
/// ```compile_fail
/// #[veoveo_types::resource_address(custom(template = "example://items/{id}", error = veoveo_types::ResourceUriError,
///     route_error = |_| veoveo_types::ResourceUriError::InvalidUri))]
/// struct Generic<T> { id: T }
/// ```
/// Component encoding helpers cannot bypass checked construction outside the owner.
/// ```compile_fail
/// mod owner {
///     #[veoveo_types::resource_address(custom(template = "example://items/{id}", error = veoveo_types::ResourceUriError,
///         route_error = |_| veoveo_types::ResourceUriError::InvalidUri))]
///     pub struct Address { id: veoveo_types::PrincipalId }
/// }
/// let id = veoveo_types::PrincipalId::parse("actor").unwrap();
/// owner::Address::resource_build_uri(&id);
/// ```
/// A cache must be private to the owner module.
/// ```compile_fail
/// #[veoveo_types::resource_address(custom(template = "example://items/{id}", error = veoveo_types::ResourceUriError,
///     route_error = |_| veoveo_types::ResourceUriError::InvalidUri))]
/// struct PublicCache {
///     id: veoveo_types::PrincipalId,
///     #[resource(cache)] pub wire: veoveo_types::ResourceUri,
/// }
/// ```
/// Private caches require private components; outside callers cannot mutate them.
/// ```compile_fail
/// mod owner {
///     #[veoveo_types::resource_address(custom(template = "example://items/{id}", error = veoveo_types::ResourceUriError,
///         route_error = |_| veoveo_types::ResourceUriError::InvalidUri))]
///     pub struct Address {
///         id: veoveo_types::PrincipalId,
///         #[resource(cache)] wire: veoveo_types::ResourceUri,
///     }
/// }
/// let mut address = owner::Address::resource_from_parts(veoveo_types::PrincipalId::parse("actor").unwrap()).unwrap();
/// address.id = veoveo_types::PrincipalId::parse("other").unwrap();
/// ```
#[derive(Debug)]
pub struct ResourceRoute {
    template: &'static str,
    root: &'static str,
    scheme: &'static str,
    authority: &'static str,
    path: &'static [RoutePath],
    query: &'static [RouteQuery],
    allow_empty_query: bool,
    trailing_slash: bool,
    /// False permits admitted query ordering and encoded-name aliases.
    canonical_query_order: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceRoutePolicy {
    pub allow_empty_query: bool,
    pub trailing_slash: bool,
    pub canonical_query_order: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceRouteError {
    Uri(ResourceUriError),
    Shape,
    Query,
    UnexpectedQuery,
    Bindings,
    Field,
    Declaration,
}
impl std::fmt::Display for ResourceRouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Uri(_) => "invalid resource URI components",
            Self::Shape => "resource URI does not match the declared route",
            Self::Query => "invalid query for the matched resource route",
            Self::UnexpectedQuery => "the matched resource route does not declare query parameters",
            Self::Bindings => "missing, duplicate or unsupported resource route binding",
            Self::Field => "invalid typed field for the matched resource route",
            Self::Declaration => "invalid resource route declaration",
        })
    }
}
impl std::error::Error for ResourceRouteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Uri(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ResourceUriError> for ResourceRouteError {
    fn from(value: ResourceUriError) -> Self {
        Self::Uri(value)
    }
}

pub enum RouteBinding<'a> {
    Scalar {
        variable: &'static str,
        value: Cow<'a, str>,
    },
    Tail {
        variable: &'static str,
        segments: Vec<Cow<'a, str>>,
    },
}

pub struct ResourceRouteMatch<'a> {
    route: &'a ResourceRoute,
    parts: &'a ResourceUriParts,
    path: Vec<Cow<'a, str>>,
}
impl ResourceRouteMatch<'_> {
    pub fn scalar(&self, variable: &str) -> Option<&str> {
        self.route
            .path
            .iter()
            .position(|part| matches!(part, RoutePath::Scalar(name) if *name == variable))
            .map(|index| self.path[index].as_ref())
    }
    pub fn tail(&self, variable: &str) -> Option<&[Cow<'_, str>]> {
        self.route
            .path
            .iter()
            .position(|part| matches!(part, RoutePath::Tail(name) if *name == variable))
            .map(|index| &self.path[index..])
    }
    pub fn query(&self, variable: &str) -> Option<&str> {
        self.route
            .query
            .iter()
            .find(|query| query.variable == variable)
            .and_then(|query| self.parts.query_parameters().get(query.name))
            .map(String::as_str)
    }
}

impl ResourceRoute {
    /// Declare a route without performing runtime work in a static initializer.
    /// Capture, build, pattern and discovery operations all validate this declaration.
    pub const fn declare(
        template: &'static str,
        root: &'static str,
        scheme: &'static str,
        authority: &'static str,
        path: &'static [RoutePath],
        query: &'static [RouteQuery],
        policy: ResourceRoutePolicy,
    ) -> Self {
        Self {
            template,
            root,
            scheme,
            authority,
            path,
            query,
            allow_empty_query: policy.allow_empty_query,
            trailing_slash: policy.trailing_slash,
            canonical_query_order: policy.canonical_query_order,
        }
    }

    /// Discovery cannot expose a template that disagrees with the component route.
    pub fn discovery_template(&self) -> Result<&'static str, ResourceRouteError> {
        self.validate()?;
        Ok(self.template)
    }

    /// Select a family by literals and arity, before parsing any domain field or query.
    /// Once selected, a query or field error must not trigger sibling-route fallback.
    pub fn matches_shape(&self, parts: &ResourceUriParts) -> bool {
        if parts.scheme() != self.scheme || parts.authority() != self.authority {
            return false;
        }
        let mut path: Vec<_> = parts.path_segments().collect();
        if self.trailing_slash {
            if !matches!(path.last(), Some(segment) if segment.is_empty()) {
                return false;
            }
            path.pop();
        }
        let tail = matches!(self.path.last(), Some(RoutePath::Tail(_)));
        if (!tail && path.len() != self.path.len()) || (tail && path.len() + 1 < self.path.len()) {
            return false;
        }
        self.path.iter().zip(&path).all(|(part, value)| match part {
            RoutePath::Literal(literal) => value.as_ref() == *literal,
            RoutePath::Scalar(_) | RoutePath::Tail(_) => true,
        })
    }

    pub fn capture<'a>(
        &'a self,
        parts: &'a ResourceUriParts,
    ) -> Result<ResourceRouteMatch<'a>, ResourceRouteError> {
        self.validate()?;
        if !self.matches_shape(parts) {
            return Err(ResourceRouteError::Shape);
        }
        if parts.has_query() && self.query.is_empty() && !self.allow_empty_query {
            return Err(ResourceRouteError::UnexpectedQuery);
        }
        if parts.has_query() && parts.query_parameters().is_empty() && !self.allow_empty_query {
            return Err(ResourceRouteError::Query);
        }
        if parts
            .query_parameters()
            .keys()
            .any(|name| !self.query.iter().any(|query| query.name == name))
        {
            return Err(ResourceRouteError::Query);
        }
        if self.canonical_query_order {
            let url =
                url::Url::parse(parts.as_str()).map_err(|_| ResourceRouteError::Declaration)?;
            let supplied: Vec<_> = url
                .query_pairs()
                .map(|(name, _)| name.into_owned())
                .collect();
            let expected: Vec<_> = self
                .query
                .iter()
                .filter(|entry| parts.query_parameters().contains_key(entry.name))
                .map(|entry| entry.name)
                .collect();
            if !supplied.iter().map(String::as_str).eq(expected) {
                return Err(ResourceRouteError::Query);
            }
        }
        let mut path: Vec<_> = parts.path_segments().collect();
        if self.trailing_slash {
            path.pop();
        }
        Ok(ResourceRouteMatch {
            route: self,
            parts,
            path,
        })
    }

    pub fn build(
        &self,
        path: &[RouteBinding<'_>],
        query: &[(&str, Cow<'_, str>)],
    ) -> Result<ResourceUri, ResourceRouteError> {
        self.validate()?;
        let variables: Vec<_> = self
            .path
            .iter()
            .filter_map(|part| match part {
                RoutePath::Scalar(name) | RoutePath::Tail(name) => Some(*name),
                _ => None,
            })
            .collect();
        if path.len() != variables.len()
            || path.iter().any(|binding| {
                let variable = match binding {
                    RouteBinding::Scalar { variable, .. } | RouteBinding::Tail { variable, .. } => {
                        variable
                    }
                };
                !variables.contains(variable)
                    || path
                        .iter()
                        .filter(|other| match other {
                            RouteBinding::Scalar {
                                variable: other, ..
                            }
                            | RouteBinding::Tail {
                                variable: other, ..
                            } => other == variable,
                        })
                        .count()
                        != 1
            })
        {
            return Err(ResourceRouteError::Bindings);
        }
        if query.iter().any(|(variable, _)| {
            !self.query.iter().any(|entry| entry.variable == *variable)
                || query.iter().filter(|(other, _)| other == variable).count() != 1
        }) {
            return Err(ResourceRouteError::Bindings);
        }
        let mut builder = ResourceUriBuilder::new(self.root)?;
        for part in self.path {
            match part {
                RoutePath::Literal(value) => builder = builder.segment(UriSegment::new(*value)?),
                RoutePath::Scalar(variable) => {
                    let Some(RouteBinding::Scalar { value, .. }) = path.iter().find(|binding| matches!(binding, RouteBinding::Scalar { variable: name, .. } if name == variable)) else { return Err(ResourceRouteError::Bindings); };
                    builder = builder.segment(UriSegment::new(value.as_ref())?);
                }
                RoutePath::Tail(variable) => {
                    let Some(RouteBinding::Tail { segments, .. }) = path.iter().find(|binding| matches!(binding, RouteBinding::Tail { variable: name, .. } if name == variable)) else { return Err(ResourceRouteError::Bindings); };
                    for segment in segments {
                        builder = builder.segment(UriSegment::new(segment.as_ref())?);
                    }
                }
            }
        }
        for declaration in self.query {
            if let Some((_, value)) = query
                .iter()
                .find(|(variable, _)| *variable == declaration.variable)
            {
                builder = builder.query_pair(declaration.name, value)?;
            }
        }
        let uri = builder.build()?;
        if !self.trailing_slash {
            return Ok(uri);
        }
        let mut url = url::Url::parse(uri.as_str()).map_err(|_| ResourceUriError::InvalidUri)?;
        url.path_segments_mut()
            .map_err(|_| ResourceUriError::NotHierarchical)?
            .push("");
        ResourceUriParts::parse(url.as_str())
            .map(ResourceUriParts::into_uri)
            .map_err(Into::into)
    }

    /// A structural pattern for the encoded wire route, not a domain-ID validator.
    pub fn wire_pattern(&self) -> Result<String, ResourceRouteError> {
        pattern::wire_pattern(self, ResourcePatternSpelling::Admitted, |_, _| None)
    }

    /// Compose explicitly encoded owner fragments with the route's wire grammar.
    /// Fragments are grouped, never decoded, rewritten or inferred from ID regexes.
    /// Regex validity and honesty about admitted aliases are the owner's obligation.
    pub fn wire_pattern_with(
        &self,
        spelling: ResourcePatternSpelling,
        fragment: impl Fn(&str, ResourcePatternContext) -> Option<ResourceEncodedPattern>,
    ) -> Result<String, ResourceRouteError> {
        pattern::wire_pattern(self, spelling, fragment)
    }

    fn validate(&self) -> Result<(), ResourceRouteError> {
        crate::ResourceTemplateUri::new(self.template)
            .map_err(|_| ResourceRouteError::Declaration)?;
        let root = ResourceUriParts::parse(self.root)?;
        if root.scheme() != self.scheme
            || root.authority() != self.authority
            || root.has_query()
            || root.path_segments().next().is_some()
        {
            return Err(ResourceRouteError::Declaration);
        }
        let mut variables = std::collections::BTreeSet::new();
        for (index, part) in self.path.iter().enumerate() {
            match part {
                RoutePath::Literal(value) => {
                    if value.contains(['/', '{', '}', '?', '#', '%'])
                        || UriSegment::new(*value).is_err()
                    {
                        return Err(ResourceRouteError::Declaration);
                    }
                }
                RoutePath::Scalar(name) | RoutePath::Tail(name) => {
                    if name.is_empty()
                        || !variables.insert(*name)
                        || (matches!(part, RoutePath::Tail(_)) && index + 1 != self.path.len())
                    {
                        return Err(ResourceRouteError::Declaration);
                    }
                }
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for query in self.query {
            if query.name.is_empty()
                || query.variable.is_empty()
                || !names.insert(query.name)
                || !variables.insert(query.variable)
            {
                return Err(ResourceRouteError::Declaration);
            }
        }
        let mut template = self.root.to_owned();
        for part in self.path {
            template.push('/');
            match part {
                RoutePath::Literal(value) => template.push_str(value),
                RoutePath::Scalar(name) => template.push_str(&format!("{{{name}}}")),
                RoutePath::Tail(name) => template.push_str(&format!("{{+{name}}}")),
            }
        }
        if self.trailing_slash {
            template.push('/');
        }
        if !self.query.is_empty() {
            template.push_str("{?");
            for (index, query) in self.query.iter().enumerate() {
                // RFC 6570 query expansion uses the variable spelling as the wire name.
                if query.name != query.variable {
                    return Err(ResourceRouteError::Declaration);
                }
                if index != 0 {
                    template.push(',');
                }
                template.push_str(query.variable);
            }
            template.push('}');
        }
        if template != self.template {
            return Err(ResourceRouteError::Declaration);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
