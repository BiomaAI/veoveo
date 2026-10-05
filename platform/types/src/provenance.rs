use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{DelegationId, PrincipalId};

/// How the current actor obtained authority to perform the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvocationMode {
    Direct,
    Delegated,
    Automated,
}

/// Provenance retained across synchronous and asynchronous execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "mode", deny_unknown_fields)]
pub enum InvocationProvenance {
    Direct {
        initiator: PrincipalId,
    },
    Delegated {
        initiator: PrincipalId,
        delegation_id: DelegationId,
    },
    Automated,
}

impl<'de> Deserialize<'de> for InvocationProvenance {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        // All Serde profiles use the existing tagged map. An empty struct
        // branch closes automated admission without changing serialization.
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case", tag = "mode", deny_unknown_fields)]
        enum TextWire {
            Direct {
                initiator: PrincipalId,
            },
            Delegated {
                initiator: PrincipalId,
                delegation_id: DelegationId,
            },
            Automated {},
        }
        Ok(match TextWire::deserialize(decoder)? {
            TextWire::Direct { initiator } => Self::Direct { initiator },
            TextWire::Delegated {
                initiator,
                delegation_id,
            } => Self::Delegated {
                initiator,
                delegation_id,
            },
            TextWire::Automated {} => Self::Automated,
        })
    }
}

impl InvocationProvenance {
    pub fn mode(&self) -> InvocationMode {
        match self {
            Self::Direct { .. } => InvocationMode::Direct,
            Self::Delegated { .. } => InvocationMode::Delegated,
            Self::Automated => InvocationMode::Automated,
        }
    }

    pub fn initiator(&self) -> Option<&PrincipalId> {
        match self {
            Self::Direct { initiator } | Self::Delegated { initiator, .. } => Some(initiator),
            Self::Automated => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn tagged_provenance_bytes_preserve_all_modes_and_reject_unknown_fields() {
        let cases = [
            json!({"mode":"direct","initiator":"pilot"}),
            json!({"mode":"delegated","initiator":"pilot","delegation_id":"delegation-native"}),
            json!({"mode":"automated"}),
        ];
        for value in cases {
            let decoded: InvocationProvenance =
                serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
            let mut unknown = value;
            unknown["undeclared"] = json!(true);
            assert!(
                serde_json::from_slice::<InvocationProvenance>(
                    &serde_json::to_vec(&unknown).unwrap()
                )
                .is_err()
            );
        }
        for value in [
            json!({"mode":"unknown"}),
            json!({"mode":"direct"}),
            json!({"mode":"delegated","initiator":"pilot"}),
            json!({"mode":"automated","initiator":"pilot"}),
        ] {
            assert!(
                serde_json::from_slice::<InvocationProvenance>(
                    &serde_json::to_vec(&value).unwrap()
                )
                .is_err()
            );
        }
    }
}
