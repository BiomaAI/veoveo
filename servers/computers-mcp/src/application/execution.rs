//! Public command admission owns no provider transport or stored bearer.
use super::{Application, ApplicationError, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_computers::{
    ComputerActor, ComputerError,
    api::{AutomationPermission, ExecuteInput},
    commands::{CommandOperation, CommandStage},
    secrets::{CommandPayload, ComputerKeyRing},
};
use veoveo_mcp_contract::PlaneCaller;
use zeroize::Zeroizing;

pub(super) struct ExecutionSupport {
    keys: Arc<ComputerKeyRing>,
    artifacts: HttpArtifactPlane,
    templates: BTreeSet<String>,
}

impl Application {
    /// Trusted installation composition. Admission stays available during a
    /// provider reconnect; the worker independently checks each dispatch.
    pub fn with_execution(
        mut self,
        keys: Arc<ComputerKeyRing>,
        artifacts: HttpArtifactPlane,
        templates: BTreeSet<String>,
    ) -> Result<Self> {
        let admitted: BTreeSet<_> = self
            .templates
            .runtimes()
            .iter()
            .map(|t| t.fingerprint())
            .collect();
        if templates.is_empty() || templates.len() > 64 || !templates.is_subset(&admitted) {
            return Err(ApplicationError::Configuration);
        }
        self.execution = Some(ExecutionSupport {
            keys,
            artifacts,
            templates,
        });
        Ok(self)
    }

    pub async fn execute(
        &self,
        caller: &PlaneCaller,
        input: ExecuteInput,
    ) -> Result<CommandOperation> {
        let actor = ComputerActor::from_verified(&caller.identity)?;
        let support = self
            .execution
            .as_ref()
            .ok_or(ApplicationError::SetupRequired)?;
        let authority = self
            .store
            .authorize_automation_grant(
                &actor,
                input.computer_id,
                input.grant_id,
                AutomationPermission::Execute,
            )
            .await?;
        if !support
            .templates
            .contains(&authority.computer()?.template_fingerprint)
        {
            return Err(ApplicationError::Unavailable);
        }
        // Bound before decoding. Native validation additionally bounds argv/env,
        // NULs and directory confinement; it owns zeroization of the moved values.
        let stdin = Zeroizing::new(input.stdin);
        if stdin.len() > 1_398_104 {
            return Err(ComputerError::InvalidInput.into());
        }
        let bytes = STANDARD
            .decode(stdin.as_bytes())
            .map_err(|_| ComputerError::InvalidInput)?;
        let request = veoveo_computer_execution::ExecutionRequest::new(
            input.arguments,
            input.directory,
            input.environment,
            bytes,
        )
        .map_err(|_| ComputerError::InvalidInput)?;
        let payload = CommandPayload::new(request, input.limits)?;
        let command = self
            .store
            .queue_command(&actor, authority, input.request_id, &payload, &support.keys)
            .await?;
        if !command.task_projected() {
            self.store.ensure_command_task(&command).await?;
        }
        if command.stage() == CommandStage::Queued
            && let Some(request) = command.output_capability_request(&support.keys)?
        {
            // After durable admission, unavailable Artifact preparation leaves
            // the same Task queued. A same-input retry can repair it while the
            // real caller is present. Never retain or remint their bearer.
            let preparation = async {
                let capability = support
                    .artifacts
                    .issue_write_capability(caller, &request)
                    .await
                    .ok()?;
                self.store
                    .attach_command_output(&command, capability, &support.keys)
                    .await
                    .ok()
            };
            if let Ok(Some(prepared)) =
                tokio::time::timeout(Duration::from_secs(5), preparation).await
            {
                self.store
                    .authorize_command_task(
                        &actor,
                        command.execution_id(),
                        veoveo_computers::commands::CommandTaskAction::Observe,
                    )
                    .await?;
                return Ok(prepared);
            }
            tracing::warn!(execution_id = %command.execution_id(), "Computer output preparation awaits an authorized retry");
        }
        self.store
            .authorize_command_task(
                &actor,
                command.execution_id(),
                veoveo_computers::commands::CommandTaskAction::Observe,
            )
            .await?;
        Ok(command)
    }
}
