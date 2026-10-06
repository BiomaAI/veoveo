//! Mutable access-request progress and public request/page values.
use crate::*;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{AccessLevel, PrincipalId, WorkContextId};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
pub enum ArtifactAccessRequestState {
    #[vocabulary(rename = "pending")]
    Pending,
    #[vocabulary(rename = "approved")]
    Approved,
    #[vocabulary(rename = "denied")]
    Denied,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum ArtifactAccessRequestDecision {
    #[vocabulary(rename = "approve")]
    Approve,
    #[vocabulary(rename = "deny")]
    Deny,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum ArtifactAccessRequestScope {
    #[vocabulary(rename = "mine")]
    Mine,
    #[vocabulary(rename = "reviewable")]
    Reviewable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateArtifactAccessRequest {
    pub requested_level: AccessLevel,
    pub justification: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecideArtifactAccessRequest {
    pub decision: ArtifactAccessRequestDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "ArtifactAccessRequest")]
pub struct ArtifactAccessRequestValue {
    pub id: ArtifactAccessRequestId,
    pub artifact_id: ArtifactId,
    pub work_context: WorkContextId,
    pub requester: PrincipalId,
    pub requested_level: AccessLevel,
    pub justification: String,
    pub state: ArtifactAccessRequestState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<DateTime<Utc>>,
}

/// Mutable progress: writers may update fields together, then validate the completed transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactAccessRequest(ArtifactAccessRequestValue);
impl veoveo_types::Check for ArtifactAccessRequestValue {
    type Error = ArtifactWireError;
    fn check(&self) -> Result<(), Self::Error> {
        let consistent = match self.state {
            ArtifactAccessRequestState::Pending => {
                self.decided_by.is_none()
                    && self.decision_note.is_none()
                    && self.decided_at.is_none()
                    && self.created_at == self.updated_at
            }
            ArtifactAccessRequestState::Approved | ArtifactAccessRequestState::Denied => {
                self.decided_by.is_some()
                    && self.decided_at == Some(self.updated_at)
                    && self.updated_at >= self.created_at
            }
            ArtifactAccessRequestState::Cancelled => {
                self.decided_by.as_ref() == Some(&self.requester)
                    && self.decision_note.is_none()
                    && self.decided_at == Some(self.updated_at)
                    && self.updated_at >= self.created_at
            }
        };
        if consistent {
            Ok(())
        } else {
            Err(ArtifactWireError::invalid(
                "artifact access request state, actor and timestamps must agree",
            ))
        }
    }
}
impl ArtifactAccessRequest {
    pub fn new(value: ArtifactAccessRequestValue) -> Result<Self, ArtifactWireError> {
        Ok(Self(veoveo_types::Checked::new(value)?.into_inner()))
    }
    pub fn validate(&self) -> Result<(), ArtifactWireError> {
        veoveo_types::Check::check(&self.0)
    }
    pub fn into_value(self) -> ArtifactAccessRequestValue {
        self.0
    }
}
impl std::ops::Deref for ArtifactAccessRequest {
    type Target = ArtifactAccessRequestValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for ArtifactAccessRequest {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Serialize for ArtifactAccessRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.validate().map_err(serde::ser::Error::custom)?;
        self.0.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ArtifactAccessRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(ArtifactAccessRequestValue::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}
impl JsonSchema for ArtifactAccessRequest {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ArtifactAccessRequest".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ArtifactAccessRequestValue::json_schema(generator)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListArtifactAccessRequests {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ArtifactAccessRequestScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ArtifactAccessRequestState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<ArtifactAccessRequestId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactAccessRequestPage {
    pub requests: Vec<ArtifactAccessRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<ArtifactAccessRequestId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    fn pending() -> ArtifactAccessRequestValue {
        let now = DateTime::parse_from_rfc3339("2026-10-06T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        ArtifactAccessRequestValue {
            id: ArtifactAccessRequestId::new(),
            artifact_id: ArtifactId::new(),
            work_context: WorkContextId::parse("review").unwrap(),
            requester: PrincipalId::parse("requester").unwrap(),
            requested_level: AccessLevel::Read,
            justification: "Research access".into(),
            state: ArtifactAccessRequestState::Pending,
            decided_by: None,
            decision_note: None,
            created_at: now,
            updated_at: now,
            decided_at: None,
        }
    }
    fn assert_rejected(value: ArtifactAccessRequestValue) {
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(ArtifactAccessRequest::new(value).is_err());
        assert!(serde_json::from_slice::<ArtifactAccessRequest>(&bytes).is_err());
    }
    #[test]
    fn progress_construction_and_decoding_share_state_actor_and_time_admission() {
        let initial = pending();
        let request = ArtifactAccessRequest::new(initial.clone()).unwrap();
        assert_eq!(
            serde_json::from_value::<ArtifactAccessRequest>(
                serde_json::to_value(&request).unwrap()
            )
            .unwrap(),
            request
        );
        for state in [
            ArtifactAccessRequestState::Approved,
            ArtifactAccessRequestState::Denied,
        ] {
            let mut decided = initial.clone();
            decided.state = state;
            decided.updated_at += TimeDelta::seconds(1);
            decided.decided_at = Some(decided.updated_at);
            decided.decided_by = Some(PrincipalId::parse("reviewer").unwrap());
            decided.decision_note = Some(String::new());
            let admitted = ArtifactAccessRequest::new(decided.clone()).unwrap();
            assert_eq!(admitted.decision_note.as_deref(), Some(""));
            assert!(
                serde_json::from_value::<ArtifactAccessRequest>(
                    serde_json::to_value(admitted).unwrap()
                )
                .is_ok()
            );
            let mut invalid = decided.clone();
            invalid.decided_by = None;
            assert_rejected(invalid);
            let mut invalid = decided.clone();
            invalid.decided_at = None;
            assert_rejected(invalid);
            let mut invalid = decided.clone();
            invalid.updated_at -= TimeDelta::seconds(2);
            invalid.decided_at = Some(invalid.updated_at);
            assert_rejected(invalid);
            let mut reopened = decided;
            reopened.state = ArtifactAccessRequestState::Pending;
            reopened.created_at = reopened.updated_at;
            reopened.decided_at = None;
            reopened.decided_by = None;
            reopened.decision_note = None;
            assert!(ArtifactAccessRequest::new(reopened).is_ok());
        }
        let mut cancelled = initial.clone();
        cancelled.state = ArtifactAccessRequestState::Cancelled;
        cancelled.decided_by = Some(cancelled.requester.clone());
        cancelled.decided_at = Some(cancelled.updated_at);
        assert!(ArtifactAccessRequest::new(cancelled.clone()).is_ok());
        cancelled.decided_by = Some(PrincipalId::parse("other").unwrap());
        assert_rejected(cancelled);
        let mut invalid = initial.clone();
        invalid.updated_at += TimeDelta::seconds(1);
        assert_rejected(invalid);
        let mut invalid = initial.clone();
        invalid.decision_note = Some(String::new());
        assert_rejected(invalid);
        let mut invalid = initial.clone();
        invalid.decided_by = Some(initial.requester);
        assert_rejected(invalid);
        let mut mutated = request;
        mutated.decided_at = Some(mutated.updated_at);
        assert!(mutated.validate().is_err());
        assert!(serde_json::to_value(mutated).is_err());
    }
}
