//! Native decoding for the retained scheduler-generation bindings.
use super::{ManagedEpisodeBinding, ManagedKernelReady};
use surrealdb::types::{Error, Kind, Object, RecordIdKey, SurrealValue, Value};

fn invalid() -> Error {
    Error::internal("invalid managed runtime binding".into())
}
fn object(value: Value, count: usize) -> Result<Object, Error> {
    match value {
        Value::Object(fields) if fields.len() == count => Ok(fields),
        _ => Err(invalid()),
    }
}
fn field<T: SurrealValue>(fields: &mut Object, name: &str) -> Result<T, Error> {
    T::from_value(fields.remove(name).ok_or_else(invalid)?)
}
impl SurrealValue for ManagedKernelReady {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        let mut fields = Object::new();
        fields.insert("generation", self.generation.into_value());
        fields.insert("pod_uid", self.pod_uid.into_value());
        Value::Object(fields)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        let mut fields = object(value, 2)?;
        let row = Self {
            generation: field(&mut fields, "generation")?,
            pod_uid: field(&mut fields, "pod_uid")?,
        };
        if row.generation <= 0 || row.pod_uid.is_nil() {
            return Err(invalid());
        }
        Ok(row)
    }
}
impl SurrealValue for ManagedEpisodeBinding {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        let mut fields = Object::new();
        fields.insert("instance", self.instance.into_value());
        fields.insert("revision", self.revision.into_value());
        fields.insert("generation", self.generation.into_value());
        fields.insert("epoch", self.epoch.into_value());
        Value::Object(fields)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        let mut fields = object(value, 4)?;
        let row = Self {
            instance: field(&mut fields, "instance")?,
            revision: field(&mut fields, "revision")?,
            generation: field(&mut fields, "generation")?,
            epoch: field(&mut fields, "epoch")?,
        };
        if row.instance.table.as_str() != "managed_agent"
            || row.revision.table.as_str() != "agent_definition_revision"
            || !matches!(row.instance.key, RecordIdKey::Uuid(_))
            || !matches!(row.revision.key, RecordIdKey::Uuid(_))
            || row.generation <= 0
            || row.epoch < 0
        {
            return Err(invalid());
        }
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use surrealdb::types::RecordId;
    use uuid::Uuid;
    #[test]
    fn readiness_and_episode_bindings_reject_unknown_fields_and_invalid_parents() {
        let ready = ManagedKernelReady {
            generation: 1,
            pod_uid: Uuid::now_v7(),
        };
        assert_eq!(
            ManagedKernelReady::from_value(ready.clone().into_value()).unwrap(),
            ready
        );
        let Value::Object(mut fields) = ready.into_value() else {
            unreachable!()
        };
        fields.insert("extra", Value::None);
        assert!(ManagedKernelReady::from_value(Value::Object(fields)).is_err());
        let binding = ManagedEpisodeBinding {
            instance: RecordId::new(
                "managed_agent",
                surrealdb::types::Uuid::from(Uuid::now_v7()),
            ),
            revision: RecordId::new(
                "agent_definition_revision",
                surrealdb::types::Uuid::from(Uuid::now_v7()),
            ),
            generation: 1,
            epoch: 0,
        };
        assert_eq!(
            ManagedEpisodeBinding::from_value(binding.clone().into_value()).unwrap(),
            binding
        );
        let invalid_binding = ManagedEpisodeBinding {
            instance: binding.revision.clone(),
            ..binding
        };
        assert!(ManagedEpisodeBinding::from_value(invalid_binding.into_value()).is_err());
    }
}
