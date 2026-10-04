use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::PrincipalKind;
use veoveo_types::{InvocationMode, ScopeName};

use super::AuthError;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct JwtClaimsWire {
    #[serde(flatten)]
    pub(super) extensions: std::collections::BTreeMap<String, serde_json::Value>,
    pub(super) iss: String,
    pub(super) sub: String,
    pub(super) principal_id: String,
    #[serde(default)]
    pub(super) principal_display_name: Option<String>,
    pub(super) client_id: String,
    #[serde(default)]
    pub(super) session_family: Option<veoveo_mcp_contract::GatewayRefreshFamilyId>,
    pub(super) work_context: String,
    pub(super) invocation_mode: InvocationMode,
    #[serde(default)]
    pub(super) initiator: Option<String>,
    #[serde(default)]
    pub(super) delegation_id: Option<String>,
    pub(super) aud: StringListClaim,
    pub(super) exp: u64,
    #[serde(default)]
    pub(super) nbf: Option<u64>,
    #[serde(default)]
    pub(super) iat: Option<u64>,
    #[serde(default)]
    pub(super) jti: Option<String>,
    #[serde(default)]
    pub(super) scope: Option<String>,
    #[serde(default)]
    pub(super) scp: Option<StringListClaim>,
    #[serde(default)]
    pub(super) groups: Option<StringListClaim>,
    #[serde(default)]
    pub(super) roles: Option<StringListClaim>,
    #[serde(default)]
    pub(super) tenant: Option<String>,
    #[serde(default)]
    pub(super) data_labels: Option<StringListClaim>,
    #[serde(default)]
    pub(super) principal_assurances: Option<StringListClaim>,
    #[serde(default)]
    pub(super) principal_kind: Option<PrincipalKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ClientAssertionClaims {
    pub(super) iss: String,
    pub(super) sub: String,
    pub(super) aud: StringListClaim,
    pub(super) exp: u64,
    pub(super) jti: String,
    #[serde(default)]
    pub(super) nbf: Option<u64>,
    #[serde(default)]
    pub(super) iat: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct IdJagClaims {
    pub(super) iss: String,
    pub(super) sub: String,
    pub(super) aud: StringListClaim,
    pub(super) exp: u64,
    pub(super) jti: String,
    pub(super) client_id: String,
    #[serde(default)]
    pub(super) resource: Option<String>,
    #[serde(default)]
    pub(super) nbf: Option<u64>,
    #[serde(default)]
    pub(super) iat: Option<u64>,
    #[serde(default)]
    pub(super) scope: Option<String>,
    #[serde(default)]
    pub(super) scp: Option<StringListClaim>,
    #[serde(default)]
    pub(super) groups: Option<StringListClaim>,
    #[serde(default)]
    pub(super) roles: Option<StringListClaim>,
    #[serde(default)]
    pub(super) tenant: Option<String>,
    #[serde(default)]
    pub(super) data_labels: Option<StringListClaim>,
    #[serde(default)]
    pub(super) principal_assurances: Option<StringListClaim>,
    #[serde(default)]
    pub(super) email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct OidcIdTokenClaims {
    pub(super) iss: String,
    pub(super) sub: String,
    pub(super) aud: StringListClaim,
    pub(super) exp: u64,
    pub(super) iat: u64,
    #[serde(default)]
    pub(super) nbf: Option<u64>,
    #[serde(default)]
    pub(super) nonce: Option<String>,
    #[serde(default)]
    pub(super) groups: Option<StringListClaim>,
    #[serde(default)]
    pub(super) roles: Option<StringListClaim>,
    #[serde(default)]
    pub(super) tenant: Option<String>,
    #[serde(default)]
    pub(super) tid: Option<String>,
    #[serde(default)]
    pub(super) oid: Option<String>,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) email: Option<String>,
    #[serde(default)]
    pub(super) preferred_username: Option<String>,
    #[serde(default)]
    pub(super) data_labels: Option<StringListClaim>,
    #[serde(default)]
    pub(super) principal_assurances: Option<StringListClaim>,
}

impl JwtClaimsWire {
    pub(super) fn scopes(&self) -> Result<BTreeSet<ScopeName>, AuthError> {
        let mut values = BTreeSet::new();
        if let Some(scope) = &self.scope {
            for item in scope.split_whitespace() {
                values.insert(ScopeName::new(item).map_err(AuthError::Claim)?);
            }
        }
        if let Some(scp) = &self.scp {
            for item in scp.values() {
                values.insert(ScopeName::new(item).map_err(AuthError::Claim)?);
            }
        }
        Ok(values)
    }
}

impl IdJagClaims {
    pub(super) fn scopes(&self) -> Result<BTreeSet<ScopeName>, AuthError> {
        let mut values = BTreeSet::new();
        if let Some(scope) = &self.scope {
            for item in scope.split_whitespace() {
                values.insert(ScopeName::new(item).map_err(AuthError::Claim)?);
            }
        }
        if let Some(scp) = &self.scp {
            for item in scp.values() {
                values.insert(ScopeName::new(item).map_err(AuthError::Claim)?);
            }
        }
        Ok(values)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum StringListClaim {
    One(String),
    Many(Vec<String>),
}

impl StringListClaim {
    pub(super) fn values(&self) -> Vec<&str> {
        match self {
            Self::One(value) => vec![value.as_str()],
            Self::Many(values) => values.iter().map(String::as_str).collect(),
        }
    }

    pub(super) fn into_values(self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize)]
pub(super) struct JwtClaims(pub JwtClaimsWire);
impl std::ops::Deref for JwtClaims {
    type Target = JwtClaimsWire;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<'de> Deserialize<'de> for JwtClaims {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = veoveo_types::UniqueJsonValue::deserialize(deserializer)?.0;
        if value.get("managed_execution").is_some() {
            return Err(serde::de::Error::custom(
                "public token cannot assert internal execution attribution",
            ));
        }
        serde_json::from_value(value)
            .map(Self)
            .map_err(|_| serde::de::Error::custom("invalid access-token claims"))
    }
}

impl std::fmt::Debug for JwtClaims {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("JwtClaims([REDACTED])")
    }
}
