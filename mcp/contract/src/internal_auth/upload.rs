//! Upload-only assertion binds the gateway decision to its checked configuration.

use super::*;
use crate::{ARTIFACT_UPLOAD_AUDIENCE, ArtifactUploadAuthority};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedArtifactUploadIdentity {
    pub identity: GatewayInternalIdentity,
    pub authorization: ArtifactUploadAuthority,
}

#[derive(Serialize, Deserialize)]
struct UploadClaims {
    #[serde(flatten)]
    identity: GatewayInternalJwtClaims,
    upload_authorization: ArtifactUploadAuthority,
}

impl GatewayInternalTokenIssuer {
    pub fn issue_artifact_upload(
        &self,
        profile: GatewayProfileId,
        actor: Principal,
        authority: InvocationAuthority,
        authorization: ArtifactUploadAuthority,
        expires_at: DateTime<Utc>,
    ) -> Result<String, InternalTokenError> {
        let identity = self.create_identity(
            profile,
            ServerSlug::new(ARTIFACT_UPLOAD_AUDIENCE).map_err(InternalTokenError::Identifier)?,
            actor,
            authority,
            None,
            expires_at,
        )?;
        let claims = UploadClaims {
            identity: GatewayInternalJwtClaims::from_identity(&identity),
            upload_authorization: authorization,
        };
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".into());
        header.kid = Some(self.signing_key.key_id.clone());
        encode(
            &header,
            &claims,
            &EncodingKey::from_ed_der(&self.signing_key.private_key_der),
        )
        .map_err(InternalTokenError::Jwt)
    }
}

impl GatewayInternalTokenVerifier {
    pub fn verify_artifact_upload(
        &self,
        token: &str,
    ) -> Result<VerifiedArtifactUploadIdentity, InternalTokenError> {
        let claims = self.decode_claims::<UploadClaims>(token)?;
        let expected =
            ServerSlug::new(ARTIFACT_UPLOAD_AUDIENCE).map_err(InternalTokenError::Identifier)?;
        if claims.identity.server != expected {
            return Err(InternalTokenError::AudienceMismatch {
                expected,
                actual: claims.identity.server,
            });
        }
        Ok(VerifiedArtifactUploadIdentity {
            identity: self.identity_from_claims(claims.identity)?,
            authorization: claims.upload_authorization,
        })
    }
}
