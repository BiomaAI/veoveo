//! SQL fields derived only from the source's checked observation.
use super::serialize_subject;
use chrono::{DateTime, Utc};
use surrealdb::types::SurrealValue;
use veoveo_mcp_knowledge_extension::{AccessDescriptor, ReadPolicy};

#[derive(Debug, PartialEq, SurrealValue)]
pub(super) struct Admission {
    pub tenant_read: bool,
    pub context_read: bool,
    pub selected_context_read: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub work_context: Option<String>,
    pub required_context: Option<String>,
    pub required_profile: Option<String>,
    pub grants: Vec<Grant>,
    pub labels: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, SurrealValue)]
pub(super) struct Grant {
    subject: String,
    expires_at: Option<DateTime<Utc>>,
}

impl From<Option<&AccessDescriptor>> for Admission {
    fn from(access: Option<&AccessDescriptor>) -> Self {
        let Some(access) = access else {
            return Self {
                tenant_read: true,
                context_read: false,
                selected_context_read: false,
                expires_at: None,
                work_context: None,
                required_context: None,
                required_profile: None,
                grants: vec![],
                labels: vec![],
            };
        };
        let mut grants: Vec<_> = access
            .grants
            .iter()
            .map(|grant| Grant {
                subject: serialize_subject(&grant.subject),
                expires_at: grant.expires_at,
            })
            .chain(std::iter::once(Grant {
                subject: serialize_subject(&access.owner),
                expires_at: None,
            }))
            .collect();
        grants.sort();
        grants.dedup();
        let (tenant_read, context_read, required_context, required_profile) =
            match &access.read_policy {
                ReadPolicy::Tenant {} => (true, false, None, None),
                ReadPolicy::Subjects {} => (false, false, None, None),
                ReadPolicy::WorkContext {} => (false, true, None, None),
                ReadPolicy::SelectedWorkContext {} => (false, false, None, None),
                ReadPolicy::SubjectsInContext { profile } => (
                    false,
                    false,
                    Some(access.work_context.to_string()),
                    profile.as_ref().map(ToString::to_string),
                ),
            };
        Self {
            tenant_read,
            context_read,
            selected_context_read: matches!(access.read_policy, ReadPolicy::SelectedWorkContext {}),
            expires_at: access.expires_at,
            work_context: Some(access.work_context.to_string()),
            required_context,
            required_profile,
            grants,
            labels: access.data_labels.iter().map(ToString::to_string).collect(),
        }
    }
}
