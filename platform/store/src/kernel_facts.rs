//! Typed facts returned by narrow current-directory kernel SQL APIs.
use serde::Deserialize;
use surrealdb::types::{Error, Kind, SurrealValue, Value};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentTenant {
    pub slug: veoveo_types::TenantId,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentPrincipal {
    pub kind: crate::PrincipalKind,
    pub issuer: veoveo_types::TokenIssuer,
    pub subject: veoveo_types::TokenSubject,
}

impl SurrealValue for CurrentTenant {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        let mut object = surrealdb::types::Object::default();
        object.insert("slug", self.slug.as_str().into_value());
        object.into_value()
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        serde_json::from_value(crate::json_value::from_surreal_json(value)?)
            .map_err(|error| Error::internal(format!("invalid current directory fact: {error}")))
    }
}

impl SurrealValue for CurrentPrincipal {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        let mut object = surrealdb::types::Object::default();
        object.insert("kind", self.kind.into_value());
        object.insert("issuer", self.issuer.as_str().into_value());
        object.insert("subject", self.subject.as_str().into_value());
        object.into_value()
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        serde_json::from_value(crate::json_value::from_surreal_json(value)?)
            .map_err(|error| Error::internal(format!("invalid current directory fact: {error}")))
    }
}
