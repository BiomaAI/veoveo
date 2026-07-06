use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{PrincipalKind, ScopeName};

use super::AuthError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct JwtClaims {
    pub(super) iss: String,
    pub(super) sub: String,
    pub(super) client_id: String,
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
    pub(super) email: Option<String>,
    #[serde(default)]
    pub(super) preferred_username: Option<String>,
    #[serde(default)]
    pub(super) data_labels: Option<StringListClaim>,
    #[serde(default)]
    pub(super) principal_assurances: Option<StringListClaim>,
}

impl JwtClaims {
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
