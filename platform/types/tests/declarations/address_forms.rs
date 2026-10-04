//! Positive address declarations exercise the same native test harness as ID forms.
use veoveo_types::{
    IdProfile, IdProfileSpec, ResourceAddress, ResourceProfile, ResourceProfileSpec,
    ResourceRouteError, ResourceSchema, ResourceUri, ResourceUriError,
};

#[doc(hidden)]
pub struct Names;
impl IdProfile for Names {
    type Error = ResourceUriError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            Err(ResourceUriError::DisallowedComponent)
        } else {
            Ok(())
        }
    });
}
#[veoveo_types::id(text(Names))]
struct Name(String);
#[doc(hidden)]
pub struct Uris;
impl ResourceProfile for Uris {
    type Error = ResourceUriError;
    const PROFILE: ResourceProfileSpec<Self::Error> = ResourceProfileSpec {
        route_error: |_, error| match error {
            ResourceRouteError::Uri(error) => error,
            _ => ResourceUriError::DisallowedComponent,
        },
    };
    const SCHEMA: Option<ResourceSchema> = Some(ResourceSchema {
        schema: |_, generator| <String as schemars::JsonSchema>::json_schema(generator),
        inline: true,
    });
}
#[veoveo_types::resource_address(cached_checked(Uris), template = "example://checked/{id}")]
#[schemars(description = "A checked address with preserved schema metadata.")]
struct Checked {
    #[resource(cache)]
    wire: String,
    #[resource(accessor = id)]
    id: Name,
}
#[veoveo_types::resource_address(cached(Uris), template = "example://cached/{id}", constructor = borrowed, from_str, resource_uri)]
struct Cached {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(accessor = id, clone_accessor)]
    id: Name,
}
#[veoveo_types::resource_address(cached(Uris), template = "example://index{?cursor}")]
struct Index {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(accessor = cursor, argument = optional_borrowed)]
    cursor: Option<Name>,
}
#[veoveo_types::resource_address(components(Uris), template = "example://component/{id}")]
struct Component(#[resource(variable = "id", accessor = id)] Name);
#[veoveo_types::resource_address(components(Uris), template = "example://pair/{first}/{second}")]
struct Pair(
    #[resource(variable = "first")] Name,
    #[resource(variable = "second")] Name,
);
#[veoveo_types::resource_address(routes(Uris), traits = none, wire)]
enum WireOnly {
    #[resource(template = "example://wire")]
    Root,
}
impl Clone for WireOnly {
    fn clone(&self) -> Self {
        Self::Root
    }
}
#[veoveo_types::resource_address(routes(Uris), wire, schema = owner, display)]
#[derive(Default)]
enum Routes {
    #[default]
    #[resource(template = "example://root")]
    Root,
    #[resource(template = "example://component/{id}")]
    Item(#[resource(variable = "id")] Name),
}
#[veoveo_types::resource_address(components(Uris), template = "example://copy/{id}", traits = copied, to_uri = copied)]
struct CopyComponent(
    #[resource(variable = "id", error = |_| ResourceUriError::DisallowedComponent, accessor = id, owned_accessor)]
     veoveo_types::TaskId,
);
fn checked_name(value: Name) -> Result<Name, ResourceUriError> {
    if value.as_str() == "rejected" {
        Err(ResourceUriError::DisallowedComponent)
    } else {
        Ok(value)
    }
}
#[veoveo_types::resource_address(cached_checked(Uris), template = "example://admitted/{id}")]
struct Admitted {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(accessor = id, admit = checked_name, codec = AdmittedNames)]
    id: Name,
}

struct AdmittedNames;
impl veoveo_types::ResourceFieldCodec<Name> for AdmittedNames {
    type Error = ResourceUriError;
    fn parse(value: &str) -> Result<Name, Self::Error> {
        checked_name(Name::parse(value)?)
    }
    fn text(value: &Name) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}

#[test]
fn owned_borrowed_optional_and_component_addresses_keep_wire_and_admission() {
    let wire_only = serde_json::to_value(WireOnly::Root).unwrap();
    assert_eq!(wire_only, "example://wire");
    assert!(matches!(
        serde_json::from_value::<WireOnly>(wire_only).unwrap(),
        WireOnly::Root
    ));
    let id = Name::parse("example").unwrap();
    assert_eq!(Routes::default(), Routes::Root);
    let checked = Checked::new(id.clone()).unwrap();
    assert_eq!(Checked::parse(checked.as_str()).unwrap(), checked);
    let cached = Cached::new(&id);
    assert_eq!(cached.id(), id);
    assert_eq!(cached.as_resource_uri().as_str(), cached.as_str());
    assert_eq!(cached.as_str().parse::<Cached>().unwrap(), cached);
    let index = Index::new(Some(&id));
    assert_eq!(index.cursor(), Some(&id));
    assert_eq!(Index::parse(index.as_str()).unwrap(), index);
    let component = Component::new(id.clone());
    // Wrapped construction preserves the typed payload without codec reparsing.
    let wrap: fn(Name) -> Component = Component::new;
    assert_eq!(wrap(id.clone()).0, id);
    let pair = Pair::new(id.clone(), Name::parse("second").unwrap());
    let wrap_pair: fn(Name, Name) -> Pair = Pair::new;
    assert_eq!(wrap_pair(pair.0.clone(), pair.1.clone()), pair);
    assert_eq!(Pair::parse(pair.to_uri().as_str()).unwrap(), pair);
    let task = veoveo_types::TaskId::new();
    let copied = CopyComponent::new(task);
    let copy_accessor: fn(CopyComponent) -> veoveo_types::TaskId = CopyComponent::id;
    assert_eq!(copy_accessor(copied), task);
    assert_eq!(
        CopyComponent::parse(copied.to_uri().as_str()).unwrap(),
        copied
    );
    assert_eq!(component.id(), &id);
    assert_eq!(
        Component::parse(component.to_uri().as_str()).unwrap(),
        component
    );
    for route in [Routes::Root, Routes::Item(id)] {
        let wire = <Routes as ResourceAddress>::to_uri(&route).unwrap();
        assert_eq!(Routes::parse(wire.as_str()).unwrap(), route);
        assert_eq!(serde_json::to_value(&route).unwrap(), wire.as_str());
        assert_eq!(
            serde_json::from_value::<Routes>(serde_json::json!(wire.as_str())).unwrap(),
            route
        );
    }
    assert!(Admitted::new(Name::parse("rejected").unwrap()).is_err());
    assert!(Admitted::parse("example://admitted/rejected").is_err());
    assert!(serde_json::from_str::<Admitted>("\"example://admitted/rejected\"").is_err());
    assert!(Admitted::new(Name::parse("accepted").unwrap()).is_ok());
    for invalid in [
        "example://component/",
        "example://component/example/extra",
        "example://component/example?unknown=1",
    ] {
        assert!(Component::parse(invalid).is_err());
    }
}

#[test]
fn owner_schema_and_derived_metadata_keep_their_explicit_profiles() {
    use schemars::JsonSchema;
    assert!(Routes::inline_schema());
    assert_eq!(Routes::schema_name(), "Routes");
    assert_eq!(Routes::schema_id(), "Routes");
    assert_eq!(
        Routes::json_schema(&mut schemars::SchemaGenerator::default()),
        <String as JsonSchema>::json_schema(&mut schemars::SchemaGenerator::default())
    );
    let schema = Checked::json_schema(&mut schemars::SchemaGenerator::default());
    assert_eq!(
        schema.get("description"),
        Some(&serde_json::json!(
            "A checked address with preserved schema metadata."
        ))
    );
    assert!(Checked::schema_id().ends_with("::Checked"));
}
