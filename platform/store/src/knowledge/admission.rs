//! SQL fields derived only from the source's checked observation.
use super::serialize_subject;
use surrealdb::types::SurrealValue;
use veoveo_mcp_knowledge_extension::{AccessDescriptor, ReadPolicy};

#[derive(Debug, PartialEq, SurrealValue)]
pub(super) struct Admission {
    pub tenant_read: bool,
    pub context_read: bool,
    pub work_context: Option<String>,
    pub required_context: Option<String>,
    pub required_profile: Option<String>,
    pub subjects: Vec<String>,
    pub labels: Vec<String>,
}

impl From<Option<&AccessDescriptor>> for Admission {
    fn from(access: Option<&AccessDescriptor>) -> Self {
        let Some(access) = access else {
            return Self {
                tenant_read: true,
                context_read: false,
                work_context: None,
                required_context: None,
                required_profile: None,
                subjects: vec![],
                labels: vec![],
            };
        };
        let mut subjects: Vec<_> = access
            .grants
            .iter()
            .chain(std::iter::once(&access.owner))
            .map(serialize_subject)
            .collect();
        subjects.sort();
        subjects.dedup();
        let (tenant_read, context_read, required_context, required_profile) =
            match &access.read_policy {
                ReadPolicy::Tenant {} => (true, false, None, None),
                ReadPolicy::Subjects {} => (false, false, None, None),
                ReadPolicy::WorkContext {} => (false, true, None, None),
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
            work_context: Some(access.work_context.to_string()),
            required_context,
            required_profile,
            subjects,
            labels: access.data_labels.iter().map(ToString::to_string).collect(),
        }
    }
}
