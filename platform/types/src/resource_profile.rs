//! Owner error projection for shared checked cached addresses.
use crate::{ResourceRouteError, ResourceUriError};

/// Owner schema declarations require a schema callback at compile time.
/// ```compile_fail
/// use veoveo_types::{ResourceProfile, ResourceProfileSpec, ResourceUriError, PrincipalId};
/// struct MissingSchema;
/// impl ResourceProfile for MissingSchema {
///     type Error = ResourceUriError;
///     const PROFILE: ResourceProfileSpec<Self::Error> = ResourceProfileSpec {
///         route_error: |_, _| ResourceUriError::DisallowedComponent,
///     };
/// }
/// #[veoveo_types::resource_address(components(MissingSchema), template = "example://item/{id}")]
/// struct Address(#[resource(variable = "id", error = |_| ResourceUriError::DisallowedComponent)] PrincipalId);
/// ```
pub trait ResourceProfile {
    type Error;
    const PROFILE: ResourceProfileSpec<Self::Error>;
    const SCHEMA: Option<ResourceSchema> = None;
    fn uri_error(name: &'static str, error: ResourceUriError) -> Self::Error {
        (Self::PROFILE.route_error)(name, ResourceRouteError::Uri(error))
    }
}
pub struct ResourceProfileSpec<E> {
    pub route_error: fn(&'static str, ResourceRouteError) -> E,
}
impl<E> Copy for ResourceProfileSpec<E> {}
impl<E> Clone for ResourceProfileSpec<E> {
    fn clone(&self) -> Self {
        *self
    }
}

/// Explicit schema policy for addresses with a nonderived public schema.
#[derive(Clone, Copy)]
pub struct ResourceSchema {
    pub schema: fn(&'static str, &mut schemars::SchemaGenerator) -> schemars::Schema,
    pub inline: bool,
}
