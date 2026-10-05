//! Frozen Computer Audit target and its driver-independent lookup projection.
use crate::ComputerId;
use veoveo_audit_contract::{
    AuditLookupKey, AuditLookupReference, AuditTargetError, AuditTargetOwner,
};
#[derive(Debug, Clone, PartialEq, Eq, schemars::JsonSchema)]
#[schemars(transform = audit_target_schema)]
#[schemars(deny_unknown_fields)]
pub struct ComputerAuditTarget {
    pub computer: ComputerId,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum ComputerTargetWire {
    #[serde(rename = "computer")]
    Computer { computer: ComputerId },
}
impl serde::Serialize for ComputerAuditTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(
            &ComputerTargetWire::Computer {
                computer: self.computer,
            },
            serializer,
        )
    }
}
impl<'de> serde::Deserialize<'de> for ComputerAuditTarget {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match <ComputerTargetWire as serde::Deserialize>::deserialize(deserializer)? {
            ComputerTargetWire::Computer { computer } => Ok(Self { computer }),
        }
    }
}
impl AuditTargetOwner for ComputerAuditTarget {
    const KIND: &'static str = "computer";
    fn lookup_reference(&self) -> Result<Option<AuditLookupReference>, AuditTargetError> {
        Ok(Some(AuditLookupReference::new(
            "computer",
            AuditLookupKey::Uuid(self.computer.as_uuid()),
        )?))
    }
}
pub fn register_audit_target(
    builder: &mut veoveo_audit_contract::AuditTargetRegistryBuilder,
) -> Result<veoveo_audit_contract::AuditTargetRegistration<ComputerAuditTarget>, AuditTargetError> {
    builder.register::<ComputerAuditTarget>()
}

fn audit_target_schema(schema: &mut schemars::Schema) {
    schema
        .as_object_mut()
        .expect("Computer target object")
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
        .expect("Computer target properties")
        .insert(
            "kind".into(),
            serde_json::json!({"type":"string","const":"computer"}),
        );
    schema
        .as_object_mut()
        .expect("Computer target object")
        .get_mut("required")
        .and_then(serde_json::Value::as_array_mut)
        .expect("Computer target required fields")
        .push(serde_json::json!("kind"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_audit_contract::*;
    #[test]
    fn frozen_computer_target_and_lookup() {
        let computer = ComputerId::parse("019f25e8-cf39-7000-8000-000000000001").unwrap();
        let mut builder = AuditTargetRegistry::builder();
        let registration = register_audit_target(&mut builder).unwrap_or_else(|error| {
            panic!(
                "{error}: {}",
                serde_json::to_value(schemars::schema_for!(ComputerAuditTarget)).unwrap()
            )
        });
        let registry = builder.build();
        let target = registration
            .target(&registry, ComputerAuditTarget { computer })
            .unwrap();
        let wire = serde_json::json!({"kind":"computer","computer":"019f25e8-cf39-7000-8000-000000000001"});
        assert_eq!(serde_json::to_value(&target).unwrap(), wire);
        let decoded: AuditTarget = registry.decoder().from_value(wire).unwrap();
        assert_eq!(decoded, target);
        assert_eq!(
            registration.get(&registry, &decoded).unwrap().computer,
            computer
        );
        if let AuditTarget::Extension(target) = target {
            assert_eq!(
                target.lookup_reference(),
                Some(
                    &AuditLookupReference::new(
                        "computer",
                        AuditLookupKey::Uuid(computer.as_uuid())
                    )
                    .unwrap()
                )
            );
        } else {
            panic!("owner target was not admitted as extension");
        }
        assert!(
            registry
                .decoder()
                .from_str::<AuditTarget>(r#"{"kind":"computer","computer":"bad"}"#)
                .is_err()
        );
        assert!(registry.decoder().from_str::<AuditTarget>(r#"{"kind":"computer","computer":"019f25e8-cf39-7000-8000-000000000001","computer":"019f25e8-cf39-7000-8000-000000000001"}"#).is_err());
    }
}
