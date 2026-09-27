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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "mode")]
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
