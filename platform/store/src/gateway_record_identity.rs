//! Verify retained gateway keys against the controlled identities they address.
use crate::*;
pub(crate) trait GatewayRecordIdentity {
    fn verify_identity(&self) -> Result<(), StoreError>;
}
fn matches(actual: &RecordId, expected: RecordId) -> Result<(), StoreError> {
    if actual == &expected {
        Ok(())
    } else {
        Err(StoreError::InvalidGatewayStateIdentity)
    }
}
impl GatewayRecordIdentity for GatewayResourceSubscriptionRecord {
    fn verify_identity(&self) -> Result<(), StoreError> {
        matches(
            &self.id,
            gateway_resource_subscription_record_id(
                self.profile.as_str(),
                self.owner.as_str(),
                self.upstream_server.as_str(),
                self.resource_uri.as_str(),
            ),
        )
    }
}
impl GatewayRecordIdentity for GatewayJwtRevocationRecord {
    fn verify_identity(&self) -> Result<(), StoreError> {
        matches(
            &self.id,
            gateway_jwt_revocation_record_id(
                self.profile.as_str(),
                self.issuer.as_str(),
                self.jwt_id.as_str(),
            ),
        )
    }
}
impl GatewayRecordIdentity for GatewayAuthorizationRequestRecord {
    fn verify_identity(&self) -> Result<(), StoreError> {
        matches(
            &self.id,
            gateway_authorization_request_record_id(self.idp_state.as_str()),
        )
    }
}
impl GatewayRecordIdentity for GatewayAuthorizationCodeStateRecord {
    fn verify_identity(&self) -> Result<(), StoreError> {
        matches(
            &self.id,
            gateway_authorization_code_record_id(self.code.as_str()),
        )
    }
}
pub(crate) fn checked<T: GatewayRecordIdentity>(
    record: Option<T>,
) -> Result<Option<T>, StoreError> {
    if let Some(record) = &record {
        record.verify_identity()?;
    }
    Ok(record)
}
