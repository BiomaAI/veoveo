//! Exact registered-consumer matching for the private Docker volume boundary.
//! A match does not allocate storage or authorize changing the admitted writer.
use crate::{Binding, Result, RuntimeFailure};
use serde::Deserialize;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedWriter {
    provider_id: Uuid,
    engine_id: Uuid,
    namespace: String,
    binding: Binding,
}

/// Selected fields from Docker Engine's registered-container list. Decode the
/// original bounded response into these types before evaluating a volume mount.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RegisteredConsumer {
    id: String,
    labels: BTreeMap<String, String>,
}

impl RetainedWriter {
    pub fn new(
        provider_id: Uuid,
        engine_id: Uuid,
        namespace: String,
        binding: Binding,
    ) -> Result<Self> {
        if provider_id.is_nil()
            || engine_id.is_nil()
            || namespace.is_empty()
            || namespace.len() > 63
            || !namespace
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(RuntimeFailure::InvalidConfiguration);
        }
        Ok(Self {
            provider_id,
            engine_id,
            namespace,
            binding,
        })
    }

    pub fn provider_id(&self) -> Uuid {
        self.provider_id
    }
    pub fn engine_id(&self) -> Uuid {
        self.engine_id
    }
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// The caller obtains `consumers` by the exact retained volume filter with
    /// `all=1`. Stopped containers reserve ownership. The daemon ID comes from
    /// that same engine; an empty or failed query never permits a mount.
    pub fn matches(&self, engine_id: Uuid, consumers: &[RegisteredConsumer]) -> bool {
        let [consumer] = consumers else {
            return false;
        };
        let labels = &consumer.labels;
        engine_id == self.engine_id
            && consumer.id.len() == 64
            && consumer
                .id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            && self.binding.labels_match(labels)
            && labels
                .get("openshell.ai/managed-by")
                .is_some_and(|value| value == "openshell")
            && labels.get("openshell.ai/sandbox-namespace") == Some(&self.namespace)
            && labels.get("openshell.ai/sandbox-name") == Some(&self.binding.name())
            && labels.get("openshell.ai/sandbox-id").is_some_and(|value| {
                !value.is_empty()
                    && value.len() <= 128
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn writer() -> RetainedWriter {
        RetainedWriter::new(
            Uuid::from_u128(100),
            Uuid::from_u128(200),
            "provider-test".into(),
            Binding::new(Uuid::from_u128(300), "a".repeat(64)).unwrap(),
        )
        .unwrap()
    }
    fn consumer(writer: &RetainedWriter) -> RegisteredConsumer {
        let mut labels = writer.binding.labels();
        labels.extend([
            ("openshell.ai/managed-by".into(), "openshell".into()),
            (
                "openshell.ai/sandbox-namespace".into(),
                writer.namespace.clone(),
            ),
            ("openshell.ai/sandbox-name".into(), writer.binding.name()),
            (
                "openshell.ai/sandbox-id".into(),
                "provider-resource-1".into(),
            ),
        ]);
        RegisteredConsumer {
            id: "f".repeat(64),
            labels,
        }
    }
    #[test]
    fn foreign_engines_instances_and_namespaces_cannot_match_an_admitted_writer() {
        let writer = writer();
        assert!(writer.matches(writer.engine_id, &[consumer(&writer)]));
        assert!(!writer.matches(Uuid::from_u128(201), &[consumer(&writer)]));
        assert!(!writer.matches(writer.engine_id, &[]));
        assert!(!writer.matches(writer.engine_id, &[consumer(&writer), consumer(&writer)]));
        for key in [
            "veoveo-computer",
            "veoveo-template",
            "veoveo-instance",
            "openshell.ai/managed-by",
            "openshell.ai/sandbox-namespace",
            "openshell.ai/sandbox-name",
            "openshell.ai/sandbox-id",
        ] {
            let mut other = consumer(&writer);
            other.labels.insert(key.into(), "".into());
            assert!(!writer.matches(writer.engine_id, &[other]), "{key}");
        }
        let replacement = RetainedWriter::new(
            writer.provider_id,
            writer.engine_id,
            writer.namespace.clone(),
            Binding::replacement(
                writer.binding.computer_id(),
                Uuid::from_u128(400),
                writer.binding.template_fingerprint().into(),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(replacement.matches(writer.engine_id, &[consumer(&replacement)]));
        assert!(!replacement.matches(writer.engine_id, &[consumer(&writer)]));
        assert!(!writer.matches(writer.engine_id, &[consumer(&replacement)]));
    }
}
