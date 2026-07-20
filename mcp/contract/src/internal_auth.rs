use std::{collections::BTreeMap, fmt, sync::Arc};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode,
    errors::Error as JwtError,
    jwk::{
        AlgorithmParameters, EllipticCurve, Jwk, JwkSet, KeyAlgorithm, KeyOperations, PublicKeyUse,
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    GatewayProfileId, IdentifierError, InvocationAuthority, JwtId, Principal, PrincipalId,
    ProtectedResourceId, ServerSlug, TokenIssuer,
};

pub const GATEWAY_INTERNAL_TOKEN_ISSUER: &str = "veoveo-internal";
pub const DEFAULT_GATEWAY_INTERNAL_SIGNING_KEY_ID: &str = "veoveo-internal-1";

#[derive(Clone, PartialEq, Eq)]
pub struct GatewayInternalSigningKey {
    key_id: String,
    private_key_der: Vec<u8>,
}

impl GatewayInternalSigningKey {
    pub fn new(
        key_id: impl Into<String>,
        private_key_der: impl Into<Vec<u8>>,
    ) -> Result<Self, InternalTokenError> {
        ensure_jwt_crypto_provider();
        let key_id = validate_key_id(key_id.into())?;
        let private_key_der = private_key_der.into();
        if private_key_der.is_empty() {
            return Err(InternalTokenError::EmptyPrivateKey);
        }
        jsonwebtoken::crypto::sign(
            b"veoveo-internal-signing-key-validation",
            &EncodingKey::from_ed_der(&private_key_der),
            Algorithm::EdDSA,
        )
        .map_err(InternalTokenError::Jwt)?;
        Ok(Self {
            key_id,
            private_key_der,
        })
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }
}

impl fmt::Debug for GatewayInternalSigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewayInternalSigningKey")
            .field("key_id", &self.key_id)
            .field("private_key_der", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct GatewayInternalTrustBundle {
    keys: Arc<BTreeMap<String, Jwk>>,
}

impl GatewayInternalTrustBundle {
    pub fn new(jwks: JwkSet) -> Result<Self, InternalTokenError> {
        ensure_jwt_crypto_provider();
        if jwks.keys.is_empty() {
            return Err(InternalTokenError::EmptyTrustBundle);
        }
        let mut keys = BTreeMap::new();
        for jwk in jwks.keys {
            validate_verification_jwk(&jwk)?;
            DecodingKey::from_jwk(&jwk).map_err(InternalTokenError::Jwt)?;
            let key_id = validate_key_id(
                jwk.common
                    .key_id
                    .clone()
                    .ok_or(InternalTokenError::MissingKeyId)?,
            )?;
            if keys.insert(key_id.clone(), jwk).is_some() {
                return Err(InternalTokenError::DuplicateKeyId(key_id));
            }
        }
        Ok(Self {
            keys: Arc::new(keys),
        })
    }

    pub fn from_json(value: &str) -> Result<Self, InternalTokenError> {
        let jwks = serde_json::from_str(value).map_err(InternalTokenError::TrustBundleJson)?;
        Self::new(jwks)
    }

    pub fn key_ids(&self) -> impl Iterator<Item = &str> {
        self.keys.keys().map(String::as_str)
    }
}

fn validate_key_id(value: String) -> Result<String, InternalTokenError> {
    if value.is_empty()
        || value.trim() != value
        || value.chars().any(|character| character.is_control())
    {
        return Err(InternalTokenError::InvalidKeyId);
    }
    Ok(value)
}

fn validate_verification_jwk(jwk: &Jwk) -> Result<(), InternalTokenError> {
    if jwk.common.key_algorithm != Some(KeyAlgorithm::EdDSA) {
        return Err(InternalTokenError::UnsupportedTrustKey);
    }
    if jwk
        .common
        .public_key_use
        .as_ref()
        .is_some_and(|usage| usage != &PublicKeyUse::Signature)
    {
        return Err(InternalTokenError::UnsupportedTrustKey);
    }
    if jwk
        .common
        .key_operations
        .as_ref()
        .is_some_and(|operations| {
            operations.is_empty()
                || operations
                    .iter()
                    .any(|operation| operation != &KeyOperations::Verify)
        })
    {
        return Err(InternalTokenError::UnsupportedTrustKey);
    }
    match &jwk.algorithm {
        AlgorithmParameters::OctetKeyPair(parameters)
            if parameters.curve == EllipticCurve::Ed25519
                && URL_SAFE_NO_PAD
                    .decode(&parameters.x)
                    .is_ok_and(|key| key.len() == 32) =>
        {
            Ok(())
        }
        _ => Err(InternalTokenError::UnsupportedTrustKey),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayInternalIdentity {
    pub issuer: TokenIssuer,
    pub profile: GatewayProfileId,
    pub server: ServerSlug,
    pub actor: Principal,
    pub authority: InvocationAuthority,
    pub jwt_id: JwtId,
    pub issued_at: DateTime<Utc>,
    pub not_before: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct IssuedGatewayInternalToken {
    pub bearer_token: String,
    pub identity: GatewayInternalIdentity,
}

impl fmt::Debug for IssuedGatewayInternalToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IssuedGatewayInternalToken")
            .field("bearer_token", &"<redacted>")
            .field("identity", &self.identity)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct GatewayInternalTokenIssuer {
    issuer: TokenIssuer,
    signing_key: GatewayInternalSigningKey,
}

impl GatewayInternalTokenIssuer {
    pub fn new(issuer: TokenIssuer, signing_key: GatewayInternalSigningKey) -> Self {
        Self {
            issuer,
            signing_key,
        }
    }

    pub fn issue(
        &self,
        profile: GatewayProfileId,
        server: ServerSlug,
        actor: Principal,
        authority: InvocationAuthority,
        expires_at: DateTime<Utc>,
    ) -> Result<IssuedGatewayInternalToken, InternalTokenError> {
        ensure_jwt_crypto_provider();
        let now = Utc::now();
        if expires_at <= now {
            return Err(InternalTokenError::ExpiredDelegation);
        }
        let jwt_id =
            JwtId::new(uuid::Uuid::new_v4().to_string()).map_err(InternalTokenError::Identifier)?;
        let identity = GatewayInternalIdentity {
            issuer: self.issuer.clone(),
            profile,
            server,
            actor,
            authority,
            jwt_id,
            issued_at: now,
            not_before: now,
            expires_at,
        };
        let claims = GatewayInternalJwtClaims::from_identity(&identity);
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = Some(self.signing_key.key_id.clone());
        let bearer_token = encode(
            &header,
            &claims,
            &EncodingKey::from_ed_der(&self.signing_key.private_key_der),
        )
        .map_err(InternalTokenError::Jwt)?;
        Ok(IssuedGatewayInternalToken {
            bearer_token,
            identity,
        })
    }

    pub fn issue_resource(
        &self,
        protected_resource: ProtectedResourceId,
        server: ServerSlug,
        actor: Principal,
        authority: InvocationAuthority,
        expires_at: DateTime<Utc>,
    ) -> Result<IssuedGatewayInternalResourceToken, InternalTokenError> {
        ensure_jwt_crypto_provider();
        let now = Utc::now();
        if expires_at <= now {
            return Err(InternalTokenError::ExpiredDelegation);
        }
        let jwt_id =
            JwtId::new(uuid::Uuid::new_v4().to_string()).map_err(InternalTokenError::Identifier)?;
        let identity = GatewayInternalResourceIdentity {
            issuer: self.issuer.clone(),
            protected_resource,
            server,
            actor,
            authority,
            jwt_id,
            issued_at: now,
            not_before: now,
            expires_at,
        };
        let claims = GatewayInternalResourceJwtClaims::from_identity(&identity);
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_owned());
        header.kid = Some(self.signing_key.key_id.clone());
        let bearer_token = encode(
            &header,
            &claims,
            &EncodingKey::from_ed_der(&self.signing_key.private_key_der),
        )
        .map_err(InternalTokenError::Jwt)?;
        Ok(IssuedGatewayInternalResourceToken {
            bearer_token,
            identity,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayInternalResourceIdentity {
    pub issuer: TokenIssuer,
    pub protected_resource: ProtectedResourceId,
    pub server: ServerSlug,
    pub actor: Principal,
    pub authority: InvocationAuthority,
    pub jwt_id: JwtId,
    pub issued_at: DateTime<Utc>,
    pub not_before: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct IssuedGatewayInternalResourceToken {
    pub bearer_token: String,
    pub identity: GatewayInternalResourceIdentity,
}

impl fmt::Debug for IssuedGatewayInternalResourceToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IssuedGatewayInternalResourceToken")
            .field("bearer_token", &"<redacted>")
            .field("identity", &self.identity)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct GatewayInternalResourceTokenVerifier {
    issuer: TokenIssuer,
    audience: ServerSlug,
    trust_bundle: GatewayInternalTrustBundle,
}

impl GatewayInternalResourceTokenVerifier {
    pub fn new(
        issuer: TokenIssuer,
        audience: ServerSlug,
        trust_bundle: GatewayInternalTrustBundle,
    ) -> Self {
        Self {
            issuer,
            audience,
            trust_bundle,
        }
    }

    pub fn verify(
        &self,
        bearer_token: &str,
    ) -> Result<GatewayInternalResourceIdentity, InternalTokenError> {
        ensure_jwt_crypto_provider();
        let header = decode_header(bearer_token).map_err(InternalTokenError::Jwt)?;
        if header.alg != Algorithm::EdDSA {
            return Err(InternalTokenError::UnsupportedTokenAlgorithm(header.alg));
        }
        let key_id = header.kid.ok_or(InternalTokenError::MissingKeyId)?;
        let jwk = self
            .trust_bundle
            .keys
            .get(&key_id)
            .ok_or_else(|| InternalTokenError::UnknownKeyId(key_id.clone()))?;
        let decoding_key = DecodingKey::from_jwk(jwk).map_err(InternalTokenError::Jwt)?;
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.algorithms = vec![Algorithm::EdDSA];
        validation.validate_nbf = true;
        validation.leeway = 0;
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[self.audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub", "iat", "nbf", "jti"]);
        let claims =
            decode::<GatewayInternalResourceJwtClaims>(bearer_token, &decoding_key, &validation)
                .map_err(InternalTokenError::Jwt)?
                .claims;
        if claims.server != self.audience {
            return Err(InternalTokenError::AudienceMismatch {
                expected: self.audience.clone(),
                actual: claims.server,
            });
        }
        if PrincipalId::new(claims.sub.clone()).map_err(InternalTokenError::Identifier)?
            != claims.actor.id
        {
            return Err(InternalTokenError::SubjectPrincipalMismatch);
        }
        Ok(GatewayInternalResourceIdentity {
            issuer: claims.iss,
            protected_resource: claims.protected_resource,
            server: claims.server,
            actor: claims.actor,
            authority: claims.authority,
            jwt_id: claims.jti,
            issued_at: timestamp_to_datetime(claims.iat, "iat")?,
            not_before: timestamp_to_datetime(claims.nbf, "nbf")?,
            expires_at: timestamp_to_datetime(claims.exp, "exp")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GatewayInternalResourceJwtClaims {
    iss: TokenIssuer,
    sub: String,
    aud: String,
    exp: i64,
    nbf: i64,
    iat: i64,
    jti: JwtId,
    protected_resource: ProtectedResourceId,
    server: ServerSlug,
    actor: Principal,
    authority: InvocationAuthority,
}

impl GatewayInternalResourceJwtClaims {
    fn from_identity(identity: &GatewayInternalResourceIdentity) -> Self {
        Self {
            iss: identity.issuer.clone(),
            sub: identity.actor.id.as_str().to_owned(),
            aud: identity.server.as_str().to_owned(),
            exp: identity.expires_at.timestamp(),
            nbf: identity.not_before.timestamp(),
            iat: identity.issued_at.timestamp(),
            jti: identity.jwt_id.clone(),
            protected_resource: identity.protected_resource.clone(),
            server: identity.server.clone(),
            actor: identity.actor.clone(),
            authority: identity.authority.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GatewayInternalTokenVerifier {
    issuer: TokenIssuer,
    audiences: Vec<ServerSlug>,
    trust_bundle: GatewayInternalTrustBundle,
}

impl GatewayInternalTokenVerifier {
    /// Verify tokens audienced to exactly one server (the common upstream case).
    pub fn new(
        issuer: TokenIssuer,
        audience: ServerSlug,
        trust_bundle: GatewayInternalTrustBundle,
    ) -> Self {
        Self {
            issuer,
            audiences: vec![audience],
            trust_bundle,
        }
    }

    /// Verify tokens audienced to any of `audiences`.
    ///
    /// The shared artifact plane uses this: a domain server forwards the
    /// gateway token it received (audienced to *that* server) on the principal's
    /// behalf, so the plane must accept the set of known upstream slugs rather
    /// than a single audience. The gateway remains the only minter, so trust is
    /// unchanged; only which forwarded audiences are honored widens.
    pub fn new_for_audiences(
        issuer: TokenIssuer,
        audiences: Vec<ServerSlug>,
        trust_bundle: GatewayInternalTrustBundle,
    ) -> Self {
        Self {
            issuer,
            audiences,
            trust_bundle,
        }
    }

    pub fn verify(
        &self,
        bearer_token: &str,
    ) -> Result<GatewayInternalIdentity, InternalTokenError> {
        ensure_jwt_crypto_provider();
        let header = decode_header(bearer_token).map_err(InternalTokenError::Jwt)?;
        if header.alg != Algorithm::EdDSA {
            return Err(InternalTokenError::UnsupportedTokenAlgorithm(header.alg));
        }
        let key_id = header.kid.ok_or(InternalTokenError::MissingKeyId)?;
        let jwk = self
            .trust_bundle
            .keys
            .get(&key_id)
            .ok_or_else(|| InternalTokenError::UnknownKeyId(key_id.clone()))?;
        let decoding_key = DecodingKey::from_jwk(jwk).map_err(InternalTokenError::Jwt)?;
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.algorithms = vec![Algorithm::EdDSA];
        validation.validate_nbf = true;
        validation.leeway = 0;
        validation.set_issuer(&[self.issuer.as_str()]);
        let audience_strs: Vec<&str> = self.audiences.iter().map(ServerSlug::as_str).collect();
        validation.set_audience(&audience_strs);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub", "iat", "nbf", "jti"]);
        let token = decode::<GatewayInternalJwtClaims>(bearer_token, &decoding_key, &validation)
            .map_err(InternalTokenError::Jwt)?;
        let claims = token.claims;
        if !self.audiences.contains(&claims.server) {
            return Err(InternalTokenError::AudienceMismatch {
                expected: self
                    .audiences
                    .first()
                    .cloned()
                    .unwrap_or_else(|| claims.server.clone()),
                actual: claims.server,
            });
        }
        if PrincipalId::new(claims.sub.clone()).map_err(InternalTokenError::Identifier)?
            != claims.actor.id
        {
            return Err(InternalTokenError::SubjectPrincipalMismatch);
        }
        Ok(GatewayInternalIdentity {
            issuer: claims.iss,
            profile: claims.profile,
            server: claims.server,
            actor: claims.actor,
            authority: claims.authority,
            jwt_id: claims.jti,
            issued_at: timestamp_to_datetime(claims.iat, "iat")?,
            not_before: timestamp_to_datetime(claims.nbf, "nbf")?,
            expires_at: timestamp_to_datetime(claims.exp, "exp")?,
        })
    }
}

fn ensure_jwt_crypto_provider() {
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GatewayInternalJwtClaims {
    iss: TokenIssuer,
    sub: String,
    aud: String,
    exp: i64,
    nbf: i64,
    iat: i64,
    jti: JwtId,
    profile: GatewayProfileId,
    server: ServerSlug,
    actor: Principal,
    authority: InvocationAuthority,
}

impl GatewayInternalJwtClaims {
    fn from_identity(identity: &GatewayInternalIdentity) -> Self {
        Self {
            iss: identity.issuer.clone(),
            sub: identity.actor.id.as_str().to_string(),
            aud: identity.server.as_str().to_string(),
            exp: identity.expires_at.timestamp(),
            nbf: identity.not_before.timestamp(),
            iat: identity.issued_at.timestamp(),
            jti: identity.jwt_id.clone(),
            profile: identity.profile.clone(),
            server: identity.server.clone(),
            actor: identity.actor.clone(),
            authority: identity.authority.clone(),
        }
    }
}

#[derive(Debug)]
pub enum InternalTokenError {
    EmptyPrivateKey,
    InvalidKeyId,
    EmptyTrustBundle,
    MissingKeyId,
    DuplicateKeyId(String),
    UnknownKeyId(String),
    UnsupportedTrustKey,
    UnsupportedTokenAlgorithm(Algorithm),
    ExpiredDelegation,
    AudienceMismatch {
        expected: ServerSlug,
        actual: ServerSlug,
    },
    SubjectPrincipalMismatch,
    InvalidTimestamp {
        claim: &'static str,
        value: i64,
    },
    Identifier(IdentifierError),
    TrustBundleJson(serde_json::Error),
    Jwt(JwtError),
}

impl fmt::Display for InternalTokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPrivateKey => f.write_str("internal signing private key is empty"),
            Self::InvalidKeyId => f.write_str("internal signing key id is empty or invalid"),
            Self::EmptyTrustBundle => f.write_str("internal trust JWKS contains no keys"),
            Self::MissingKeyId => f.write_str("internal token or trust key is missing kid"),
            Self::DuplicateKeyId(key_id) => {
                write!(f, "internal trust JWKS contains duplicate kid `{key_id}`")
            }
            Self::UnknownKeyId(key_id) => {
                write!(f, "internal token references unknown kid `{key_id}`")
            }
            Self::UnsupportedTrustKey => {
                f.write_str("internal trust JWKS must contain Ed25519 verification keys")
            }
            Self::UnsupportedTokenAlgorithm(algorithm) => {
                write!(f, "internal token algorithm `{algorithm:?}` is not EdDSA")
            }
            Self::ExpiredDelegation => {
                f.write_str("internal token expiration is not in the future")
            }
            Self::AudienceMismatch { expected, actual } => write!(
                f,
                "internal token audience mismatch: expected `{expected}`, got `{actual}`"
            ),
            Self::SubjectPrincipalMismatch => {
                f.write_str("internal token subject does not match embedded principal")
            }
            Self::InvalidTimestamp { claim, value } => {
                write!(
                    f,
                    "internal token claim `{claim}` has invalid timestamp `{value}`"
                )
            }
            Self::Identifier(err) => write!(f, "invalid internal token identifier: {err}"),
            Self::TrustBundleJson(err) => write!(f, "invalid internal trust JWKS: {err}"),
            Self::Jwt(err) => write!(f, "internal token JWT validation failed: {err}"),
        }
    }
}

impl std::error::Error for InternalTokenError {}

fn timestamp_to_datetime(
    value: i64,
    claim: &'static str,
) -> Result<DateTime<Utc>, InternalTokenError> {
    DateTime::from_timestamp(value, 0).ok_or(InternalTokenError::InvalidTimestamp { claim, value })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::TimeDelta;

    use super::*;
    use crate::{GroupId, PrincipalKind, RoleId, ScopeName, TenantId, TokenSubject};

    const PRIVATE_KEY_DER_B64: &str =
        "MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl";
    const PUBLIC_KEY_X: &str = "OMOoJJu_AQS7UM8u2GVtMVj8W1zcE6QhR0DMBr9HEcg";

    fn signing_key(key_id: &str) -> GatewayInternalSigningKey {
        use base64::{Engine as _, engine::general_purpose::STANDARD};

        GatewayInternalSigningKey::new(key_id, STANDARD.decode(PRIVATE_KEY_DER_B64).unwrap())
            .unwrap()
    }

    fn trust_bundle(key_id: &str) -> GatewayInternalTrustBundle {
        GatewayInternalTrustBundle::from_json(&format!(
            r#"{{"keys":[{{"kty":"OKP","crv":"Ed25519","x":"{PUBLIC_KEY_X}","alg":"EdDSA","use":"sig","kid":"{key_id}"}}]}}"#
        ))
        .unwrap()
    }

    fn principal() -> Principal {
        Principal {
            id: PrincipalId::new("https://idp.example.com#user-1").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
            subject: TokenSubject::new("user-1").unwrap(),
            tenant: Some(TenantId::new("tenant-a").unwrap()),
            groups: BTreeSet::from([GroupId::new("engineering").unwrap()]),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::from([RoleId::new("operator").unwrap()]),
            scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: Some(Utc::now()),
        }
    }

    fn authority() -> InvocationAuthority {
        use crate::{
            AccessSubject, InvocationProvenance, PolicyVersion, WorkContextId,
            WorkContextMembershipLevel, WorkContextOutputPolicy,
        };

        InvocationAuthority {
            work_context: WorkContextId::new("mission").unwrap(),
            tenant: TenantId::new("tenant-a").unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::new("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(principal().id),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: principal().id,
            },
        }
    }

    #[test]
    fn rejects_non_eddsa_trust_keys() {
        assert!(matches!(
            GatewayInternalTrustBundle::from_json(
                r#"{"keys":[{"kty":"oct","k":"c2VjcmV0","alg":"HS256","kid":"bad"}]}"#
            ),
            Err(InternalTokenError::UnsupportedTrustKey)
        ));
    }

    #[test]
    fn internal_token_round_trips_typed_identity() {
        let issuer = GatewayInternalTokenIssuer::new(
            TokenIssuer::new("veoveo-internal").unwrap(),
            signing_key("key-1"),
        );
        let issued = issuer
            .issue(
                GatewayProfileId::new("default").unwrap(),
                ServerSlug::new("media").unwrap(),
                principal(),
                authority(),
                Utc::now() + TimeDelta::minutes(5),
            )
            .unwrap();
        assert!(!format!("{issued:?}").contains(&issued.bearer_token));

        let verified = GatewayInternalTokenVerifier::new(
            TokenIssuer::new("veoveo-internal").unwrap(),
            ServerSlug::new("media").unwrap(),
            trust_bundle("key-1"),
        )
        .verify(&issued.bearer_token)
        .unwrap();

        assert_eq!(verified.profile.as_str(), "default");
        assert_eq!(verified.server.as_str(), "media");
        assert_eq!(verified.actor.id.as_str(), "https://idp.example.com#user-1");
    }

    #[test]
    fn internal_resource_token_round_trips_without_a_synthetic_profile() {
        let issuer = GatewayInternalTokenIssuer::new(
            TokenIssuer::new("veoveo-internal").unwrap(),
            signing_key("key-1"),
        );
        let issued = issuer
            .issue_resource(
                ProtectedResourceId::new("https://veoveo.example/ingest/recordings").unwrap(),
                ServerSlug::new("recording-hub").unwrap(),
                principal(),
                authority(),
                Utc::now() + TimeDelta::minutes(5),
            )
            .unwrap();
        assert!(!format!("{issued:?}").contains(&issued.bearer_token));

        let verified = GatewayInternalResourceTokenVerifier::new(
            TokenIssuer::new("veoveo-internal").unwrap(),
            ServerSlug::new("recording-hub").unwrap(),
            trust_bundle("key-1"),
        )
        .verify(&issued.bearer_token)
        .unwrap();
        assert_eq!(
            verified.protected_resource.as_str(),
            "https://veoveo.example/ingest/recordings"
        );
        assert_eq!(verified.server.as_str(), "recording-hub");
    }

    #[test]
    fn internal_token_rejects_wrong_server_audience() {
        let issuer = GatewayInternalTokenIssuer::new(
            TokenIssuer::new("veoveo-internal").unwrap(),
            signing_key("key-1"),
        );
        let issued = issuer
            .issue(
                GatewayProfileId::new("default").unwrap(),
                ServerSlug::new("media").unwrap(),
                principal(),
                authority(),
                Utc::now() + TimeDelta::minutes(5),
            )
            .unwrap();

        let err = GatewayInternalTokenVerifier::new(
            TokenIssuer::new("veoveo-internal").unwrap(),
            ServerSlug::new("simulation").unwrap(),
            trust_bundle("key-1"),
        )
        .verify(&issued.bearer_token)
        .expect_err("wrong server audience should be rejected");

        assert!(matches!(err, InternalTokenError::Jwt(_)));
    }

    #[test]
    fn internal_token_rejects_unknown_key_id() {
        let issuer = GatewayInternalTokenIssuer::new(
            TokenIssuer::new("veoveo-internal").unwrap(),
            signing_key("retired-key"),
        );
        let issued = issuer
            .issue(
                GatewayProfileId::new("default").unwrap(),
                ServerSlug::new("media").unwrap(),
                principal(),
                authority(),
                Utc::now() + TimeDelta::minutes(5),
            )
            .unwrap();

        let err = GatewayInternalTokenVerifier::new(
            TokenIssuer::new("veoveo-internal").unwrap(),
            ServerSlug::new("media").unwrap(),
            trust_bundle("active-key"),
        )
        .verify(&issued.bearer_token)
        .expect_err("unknown kid should be rejected");

        assert!(matches!(err, InternalTokenError::UnknownKeyId(key) if key == "retired-key"));
    }

    #[test]
    fn trust_bundle_accepts_rotation_set() {
        let bundle = GatewayInternalTrustBundle::from_json(&format!(
            r#"{{"keys":[
                {{"kty":"OKP","crv":"Ed25519","x":"{PUBLIC_KEY_X}","alg":"EdDSA","use":"sig","kid":"key-1"}},
                {{"kty":"OKP","crv":"Ed25519","x":"{PUBLIC_KEY_X}","alg":"EdDSA","use":"sig","kid":"key-2"}}
            ]}}"#
        ))
        .unwrap();

        assert_eq!(bundle.key_ids().collect::<Vec<_>>(), vec!["key-1", "key-2"]);
    }

    #[test]
    fn trust_bundle_rejects_malformed_public_key() {
        assert!(matches!(
            GatewayInternalTrustBundle::from_json(
                r#"{"keys":[{"kty":"OKP","crv":"Ed25519","x":"eA","alg":"EdDSA","use":"sig","kid":"key-1"}]}"#
            ),
            Err(InternalTokenError::UnsupportedTrustKey)
        ));
    }
}
