use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use super::{
    AgentContent, AgentDefinitionMutation, AgentExecution, AgentManagementError,
    AgentTemplateParameter, Result,
};

pub(super) fn text(value: &str, field: &'static str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(|c| c == '\0') {
        return Err(AgentManagementError::Invalid(field));
    }
    Ok(())
}

pub fn key(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(AgentManagementError::Invalid("key"));
    }
    Ok(())
}

pub fn digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(AgentManagementError::Invalid("digest"));
    }
    Ok(())
}

impl AgentContent {
    pub fn validate(&self) -> Result<()> {
        key(&self.model.id)?;
        digest(&self.model.revision)?;
        text(&self.instructions, "instructions", 16_384)?;
        if self.tools.len() > 64
            || self.tools.iter().collect::<BTreeSet<_>>().len() != self.tools.len()
        {
            return Err(AgentManagementError::Invalid("tools"));
        }
        for tool in &self.tools {
            text(tool, "tool", 256)?;
            let Some((server, name)) = tool.split_once("__") else {
                return Err(AgentManagementError::Invalid("tool"));
            };
            key(server)?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
            {
                return Err(AgentManagementError::Invalid("tool"));
            }
        }
        let budget = &self.budgets;
        if !(1..=8192).contains(&budget.max_output_tokens)
            || !(1..=32).contains(&budget.max_completion_calls)
            || budget.max_tool_calls > 64
            || !(1..=900).contains(&budget.deadline_seconds)
        {
            return Err(AgentManagementError::Invalid("budgets"));
        }
        if let AgentExecution::Managed {
            template,
            template_revision,
            parameters,
            resource_subscriptions,
        } = &self.execution
        {
            key(template)?;
            digest(template_revision)?;
            if parameters.len() > 32
                || resource_subscriptions.len() > 128
                || resource_subscriptions.iter().collect::<BTreeSet<_>>().len()
                    != resource_subscriptions.len()
            {
                return Err(AgentManagementError::Invalid("template parameters"));
            }
            for (name, value) in parameters {
                key(name)?;
                if let AgentTemplateParameter::Text(value) = value {
                    text(value, "template parameter", 2048)?;
                }
            }
            for uri in resource_subscriptions {
                text(uri, "subscription", 2048)?;
                if url::Url::parse(uri).is_err() {
                    return Err(AgentManagementError::Invalid("subscription"));
                }
            }
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        hash(self)
    }
}

pub(super) fn hash(value: &impl serde::Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| AgentManagementError::Unavailable)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub(super) fn mutation(value: &AgentDefinitionMutation) -> Result<()> {
    match value {
        AgentDefinitionMutation::Create {
            name,
            description,
            content,
        } => {
            text(name, "name", 200)?;
            text(description, "description", 2000)?;
            content.validate()
        }
        AgentDefinitionMutation::Draft { content } => content.validate(),
        AgentDefinitionMutation::Publish {
            digest: value,
            audience,
        } => {
            digest(value)?;
            if audience.is_empty()
                || audience.len() > 64
                || audience
                    .iter()
                    .map(|a| &a.work_context)
                    .collect::<BTreeSet<_>>()
                    .len()
                    != audience.len()
            {
                return Err(AgentManagementError::Invalid("audience"));
            }
            for context in audience {
                digest(&context.context_digest)?;
            }
            Ok(())
        }
        AgentDefinitionMutation::Metadata { name, description } => {
            text(name, "name", 200)?;
            text(description, "description", 2000)
        }
        AgentDefinitionMutation::Transfer { owner } if owner.table.as_str() != "principal" => {
            Err(AgentManagementError::Invalid("owner"))
        }
        _ => Ok(()),
    }
}

/// Every repository result declares its validation at the typed driver boundary.
pub(super) trait StoredProjection {
    fn validate_stored(&self) -> Result<()>;
}
impl<T: StoredProjection> StoredProjection for Vec<T> {
    fn validate_stored(&self) -> Result<()> {
        for row in self {
            row.validate_stored()?;
        }
        Ok(())
    }
}
impl<T: StoredProjection> StoredProjection for Option<T> {
    fn validate_stored(&self) -> Result<()> {
        if let Some(row) = self {
            row.validate_stored()?;
        }
        Ok(())
    }
}
impl StoredProjection for super::AgentDefinition {
    fn validate_stored(&self) -> Result<()> {
        self.draft.validate()?;
        if self.draft_execution != self.draft.execution
            || self.draft_model != self.draft.model
            || self.draft_tools != self.draft.tools
            || self.draft_digest != self.draft.digest()?
        {
            return Err(AgentManagementError::Invalid("stored draft projections"));
        }
        Ok(())
    }
}
impl StoredProjection for super::AgentRevision {
    fn validate_stored(&self) -> Result<()> {
        self.content.validate()?;
        let template_revision = match &self.content.execution {
            AgentExecution::Chat => None,
            AgentExecution::Managed {
                template_revision, ..
            } => Some(template_revision.clone()),
        };
        if self.execution != self.content.execution
            || self.model != self.content.model
            || self.tools != self.content.tools
            || self.template_revision != template_revision
            || self.digest != self.content.digest()?
        {
            return Err(AgentManagementError::Invalid("stored revision projections"));
        }
        Ok(())
    }
}
impl StoredProjection for super::AgentExecutable {
    fn validate_stored(&self) -> Result<()> {
        self.revision.validate_stored()
    }
}
// These typed records carry managed progress, rather than copied opaque content.
impl StoredProjection for super::instances::ManagedAgentInstance {
    fn validate_stored(&self) -> Result<()> {
        if self.admission_count < 0 {
            return Err(AgentManagementError::Invalid("stored admission count"));
        }
        Ok(())
    }
}
impl StoredProjection for super::instances::ManagedAgentOperation {
    fn validate_stored(&self) -> Result<()> {
        Ok(())
    }
}

/// Admit field topology before any typed projection can hide undeclared fields.
/// Native identities and scalars keep the driver's decoding; open typed maps keep
/// their admitted keys in the encoding and therefore remain open.
pub(super) fn decode_stored<T: surrealdb::types::SurrealValue>(
    value: surrealdb::types::Value,
) -> Result<T> {
    let row = T::from_value(value.clone()).map_err(|_| AgentManagementError::Unavailable)?;
    let encoded = row.into_value();
    check_stored_fields(&value, &encoded)?;
    T::from_value(encoded).map_err(|_| AgentManagementError::Unavailable)
}

fn check_stored_fields(
    stored: &surrealdb::types::Value,
    typed: &surrealdb::types::Value,
) -> Result<()> {
    use surrealdb::types::Value;
    match (stored, typed) {
        (Value::Object(stored), Value::Object(typed)) => {
            for (name, value) in stored.iter() {
                // The schema may materialize the managed variant's declared optional
                // fields as NONE on a chat object. NULL is a concrete value.
                if typed.get("kind") == Some(&Value::String("chat".into()))
                    && matches!(
                        name.as_str(),
                        "template" | "template_revision" | "parameters" | "resource_subscriptions"
                    )
                    && matches!(value, Value::None)
                {
                    continue;
                }
                let admitted = typed.get(name).ok_or(AgentManagementError::Unavailable)?;
                check_stored_fields(value, admitted)?;
            }
        }
        (Value::Array(stored), Value::Array(typed)) => {
            if stored.len() != typed.len() {
                return Err(AgentManagementError::Unavailable);
            }
            for (value, admitted) in stored.iter().zip(typed.iter()) {
                check_stored_fields(value, admitted)?;
            }
        }
        _ => {}
    }
    Ok(())
}

impl StoredProjection for super::instances::ManagedAgentRegistration {
    fn validate_stored(&self) -> Result<()> {
        self.instance.validate_stored()?;
        self.revision.validate_stored()
    }
}
impl StoredProjection for super::instances::ManagedAgentReconciliation {
    fn validate_stored(&self) -> Result<()> {
        self.instance.validate_stored()?;
        self.revision.validate_stored()
    }
}
impl StoredProjection for bool {
    fn validate_stored(&self) -> Result<()> {
        Ok(())
    }
}
impl StoredProjection for i64 {
    fn validate_stored(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    use surrealdb::types::{SurrealValue, Value};

    fn content() -> AgentContent {
        AgentContent {
            model: super::super::AgentModelReference {
                id: "model".into(),
                revision: "a".repeat(64),
            },
            instructions: "Instruction".into(),
            tools: vec![],
            budgets: super::super::AgentBudgets {
                max_output_tokens: 1,
                max_completion_calls: 1,
                max_tool_calls: 0,
                deadline_seconds: 1,
            },
            execution: AgentExecution::Chat,
        }
    }

    #[test]
    fn controlled_native_objects_reject_unknown_root_nested_and_array_fields() {
        for path in [vec![], vec!["model"], vec!["budgets"], vec!["execution"]] {
            for unknown in [true.into_value(), Value::Null, Value::None] {
                let mut value = content().into_value();
                let mut object = &mut value;
                for key in &path {
                    let Value::Object(fields) = object else {
                        unreachable!()
                    };
                    object = fields.get_mut(*key).unwrap();
                }
                let Value::Object(fields) = object else {
                    unreachable!()
                };
                fields.insert("undeclared", unknown);
                assert!(
                    decode_stored::<AgentContent>(value.clone()).is_err(),
                    "{path:?}"
                );
                assert!(
                    decode_stored::<Vec<AgentContent>>(Value::Array(vec![value].into())).is_err()
                );
            }
        }
    }

    #[test]
    fn managed_controls_reject_unknown_native_fields() {
        use crate::persistence::instances::{
            ManagedAgentIdentity, ManagedAgentPublicKey, ManagedAgentResources,
        };
        fn rejects<T: SurrealValue>(value: T) {
            let value = value.into_value();
            for unknown in [true.into_value(), Value::Null, Value::None] {
                let mut stored = value.clone();
                let Value::Object(fields) = &mut stored else {
                    unreachable!()
                };
                fields.insert("undeclared", unknown);
                assert!(decode_stored::<T>(stored).is_err());
            }
            assert!(decode_stored::<T>(value).is_ok());
        }
        rejects(ManagedAgentPublicKey {
            kid: "key".into(),
            n: "modulus".into(),
            e: "exponent".into(),
        });
        rejects(ManagedAgentResources {
            namespace: "agents".into(),
            workload: "pilot".into(),
            credential_secret: "key".into(),
            volume_claim: "data".into(),
            template_config_map: "template".into(),
            image: "image".into(),
            storage_gib: 2,
        });
        rejects(ManagedAgentIdentity {
            client_id: "pilot".into(),
            issuer: "issuer".into(),
            authorization_server: "gateway".into(),
            profile: "pilot".into(),
            resource: "resource".into(),
            scopes: vec![],
            roles: vec![],
            membership: veoveo_platform_store::WorkContextMembershipLevel::Contributor,
        });
    }

    #[test]
    fn chat_inactive_none_fields_and_open_managed_parameters_keep_native_semantics() {
        let mut value = content().into_value();
        let Value::Object(fields) = &mut value else {
            unreachable!()
        };
        let Value::Object(execution) = fields.get_mut("execution").unwrap() else {
            unreachable!()
        };
        for field in [
            "template",
            "template_revision",
            "parameters",
            "resource_subscriptions",
        ] {
            execution.insert(field, Value::None);
        }
        assert_eq!(
            decode_stored::<AgentContent>(value.clone()).unwrap(),
            content()
        );
        let Value::Object(fields) = &mut value else {
            unreachable!()
        };
        let Value::Object(execution) = fields.get_mut("execution").unwrap() else {
            unreachable!()
        };
        execution.insert("parameters", Value::Null);
        assert!(decode_stored::<AgentContent>(value).is_err());
        let mut managed = content();
        managed.execution = AgentExecution::Managed {
            template: "template".into(),
            template_revision: "b".repeat(64),
            parameters: [
                (
                    "arbitrary".into(),
                    AgentTemplateParameter::Integer(i64::MAX),
                ),
                ("other".into(), AgentTemplateParameter::Boolean(true)),
            ]
            .into(),
            resource_subscriptions: vec![],
        };
        assert_eq!(
            decode_stored::<AgentContent>(managed.clone().into_value()).unwrap(),
            managed
        );
        assert!(
            decode_stored::<Option<AgentContent>>(Value::None)
                .unwrap()
                .is_none()
        );
        assert!(decode_stored::<Option<AgentContent>>(Value::Null).is_err());
    }
}

#[cfg(test)]
mod wire_profile_tests {
    use super::*;
    #[test]
    fn native_content_digest_keeps_internal_field_bytes() {
        let original = serde_json::json!({
            "model":{"id":"approved","revision":"a".repeat(64)},"instructions":"inspect",
            "tools":["time__resolve_time"],
            "budgets":{"max_output_tokens":64,"max_completion_calls":1,"max_tool_calls":2,"deadline_seconds":30},
            "execution":{"kind":"managed","template":"pilot","template_revision":"b".repeat(64),"parameters":{"vehicle":"vehicle-one"},"resource_subscriptions":[]}
        });
        let content: AgentContent = serde_json::from_value(original.clone()).unwrap();
        assert_eq!(serde_json::to_value(&content).unwrap(), original);
        let bytes = serde_json::to_vec(&content).unwrap();
        assert_eq!(
            content.digest().unwrap(),
            hex::encode(Sha256::digest(&bytes))
        );
        assert!(
            String::from_utf8(bytes)
                .unwrap()
                .contains("\"resource_subscriptions\"")
        );
        let mut public_spelling = original;
        let execution = public_spelling["execution"].as_object_mut().unwrap();
        let value = execution.remove("template_revision").unwrap();
        execution.insert("templateRevision".into(), value);
        assert!(serde_json::from_value::<AgentContent>(public_spelling).is_err());
    }
}
