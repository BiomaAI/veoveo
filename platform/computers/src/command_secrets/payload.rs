use crate::{ComputerError, Result, api::AutomationExecutionLimits};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use uuid::Uuid;
use veoveo_computer_execution::{ExecutionRequest, MAX_FRAME_BYTES, read_frame};
use zeroize::Zeroizing;

pub(super) const MAX_PLAINTEXT: usize = MAX_FRAME_BYTES + 12;

/// Immutable identities authenticated with the command.
/// Actor and owner keys are distinct domain-generated identity digests.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandBinding {
    pub execution_id: Uuid,
    pub request_id: Uuid,
    pub computer_id: Uuid,
    pub grant_id: Uuid,
    pub provider_instance_id: Uuid,
    pub owner_key: String,
    pub actor_key: String,
    pub template_fingerprint: String,
    pub resource_id: String,
    pub process_id: String,
    pub required_output_labels: std::collections::BTreeSet<veoveo_mcp_contract::DataLabelId>,
    /// Absence is the initial instance. A replacement is part of authenticated
    /// command identity; original envelopes keep their canonical initial encoding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement_instance_id: Option<Uuid>,
}
impl CommandBinding {
    pub fn instance_id(&self) -> Uuid {
        self.replacement_instance_id.unwrap_or(self.computer_id)
    }
    pub(super) fn aad(&self) -> Result<Vec<u8>> {
        let hash = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        let native_id =
            |s: &str| !s.is_empty() && s.len() <= 256 && s.bytes().all(|b| b.is_ascii_graphic());
        if [
            self.execution_id,
            self.request_id,
            self.computer_id,
            self.grant_id,
            self.provider_instance_id,
        ]
        .iter()
        .any(Uuid::is_nil)
            || !hash(&self.owner_key)
            || !hash(&self.actor_key)
            || !hash(&self.template_fingerprint)
            || !native_id(&self.resource_id)
            || !native_id(&self.process_id)
            || self.required_output_labels.len() > 256
            || self
                .replacement_instance_id
                .is_some_and(|id| id.is_nil() || id == self.computer_id)
        {
            return Err(ComputerError::InvalidInput);
        }
        serde_json::to_vec(&("veoveo.computer.command-envelope.v1", self))
            .map_err(|_| ComputerError::Unavailable)
    }
}

/// No Debug, Display or Clone: arguments, environment and stdin may contain secrets.
///
/// ```compile_fail
/// use veoveo_computers::command_secrets::CommandPayload;
/// fn cannot_log(command: CommandPayload) { let _ = format!("{command:?}"); }
/// ```
pub struct CommandPayload {
    request: ExecutionRequest,
    limits: AutomationExecutionLimits,
}
impl CommandPayload {
    pub fn new(request: ExecutionRequest, limits: AutomationExecutionLimits) -> Result<Self> {
        if !(1..=7200).contains(&limits.maximum_seconds)
            || !(1..=67108864).contains(&limits.maximum_output_bytes)
        {
            return Err(ComputerError::InvalidInput);
        }
        Ok(Self { request, limits })
    }
    pub fn request(&self) -> &ExecutionRequest {
        &self.request
    }
    pub fn limits(&self) -> AutomationExecutionLimits {
        self.limits
    }

    pub(super) fn encode(&self) -> Result<Zeroizing<Vec<u8>>> {
        // Envelope v1 admits exactly this interruption scope. Adding another
        // profile requires an explicit encoding decision, never silent omission.
        match self.limits.on_interruption {
            crate::api::AutomationInterruption::StopComputer => (),
        }
        let frame = self
            .request
            .encode()
            .map_err(|_| ComputerError::InvalidInput)?;
        let mut bytes = Zeroizing::new(Vec::with_capacity(frame.len() + 8));
        bytes.extend_from_slice(&self.limits.maximum_seconds.to_be_bytes());
        bytes.extend_from_slice(&self.limits.maximum_output_bytes.to_be_bytes());
        bytes.extend_from_slice(&frame);
        Ok(bytes)
    }
    pub(super) fn decode(bytes: &[u8]) -> Result<Self> {
        if !(12..=MAX_PLAINTEXT).contains(&bytes.len()) {
            return Err(ComputerError::Unavailable);
        }
        let word = |offset| -> Result<u32> {
            Ok(u32::from_be_bytes(
                bytes[offset..offset + 4]
                    .try_into()
                    .map_err(|_| ComputerError::Unavailable)?,
            ))
        };
        let limits = AutomationExecutionLimits {
            maximum_seconds: word(0)?,
            maximum_output_bytes: word(4)?,
            on_interruption: crate::api::AutomationInterruption::StopComputer,
        };
        let mut cursor = Cursor::new(&bytes[8..]);
        let request = read_frame(&mut cursor).map_err(|_| ComputerError::Unavailable)?;
        if cursor.position() as usize != bytes.len() - 8 {
            return Err(ComputerError::Unavailable);
        }
        Self::new(request, limits).map_err(|_| ComputerError::Unavailable)
    }
}
