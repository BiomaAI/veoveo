use super::{ComputersMcp, auth, resources};
use rmcp::{ErrorData, RoleServer, service::RequestContext};
use std::time::Instant;
use veoveo_computers::{ComputerActor, ComputerError, commands::CommandTaskAction};
use veoveo_task_runtime::TaskOwner;

pub(super) struct TaskAccess {
    pub owner: TaskOwner,
    pub deadline: Instant,
}
impl TaskAccess {
    pub async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, ErrorData>>,
    ) -> Result<T, ErrorData> {
        if Instant::now() >= self.deadline {
            return Err(auth::forbidden());
        }
        let result =
            tokio::time::timeout_at(tokio::time::Instant::from_std(self.deadline), operation)
                .await
                .map_err(|_| auth::forbidden())??;
        if Instant::now() >= self.deadline {
            return Err(auth::forbidden());
        }
        Ok(result)
    }
}
fn error(error: ComputerError) -> ErrorData {
    match error {
        ComputerError::NotFound => ErrorData::invalid_params("unknown task", None),
        ComputerError::Forbidden => auth::forbidden(),
        _ => auth::unavailable(),
    }
}
impl ComputersMcp {
    pub(super) async fn task_access(
        &self,
        context: &RequestContext<RoleServer>,
        id: &str,
        cancel: bool,
    ) -> Result<TaskAccess, ErrorData> {
        self.task_access_for_actor(&auth::actor(context)?, id, cancel)
            .await
    }
    pub(super) async fn task_access_for_actor(
        &self,
        actor: &ComputerActor,
        id: &str,
        cancel: bool,
    ) -> Result<TaskAccess, ErrorData> {
        let id = resources::canonical_uuid(id)
            .ok_or_else(|| ErrorData::invalid_params("unknown task", None))?;
        match self
            .app
            .store
            .authorize_operation_task(actor, id, cancel)
            .await
        {
            Ok(access) => Ok(TaskAccess {
                owner: access.operation().map_err(error)?.actor.clone(),
                deadline: access.valid_until(),
            }),
            Err(ComputerError::NotFound) => {
                let started = Instant::now();
                match self.app.store.maintenance(actor.owner(), id).await {
                    Ok(operation) => {
                        let control = self
                            .app
                            .store
                            .control_authority(actor)
                            .await
                            .map_err(error)?;
                        control
                            .require_read(Some(operation.computer_id))
                            .map_err(error)?;
                        if cancel {
                            control.require_update_template().map_err(error)?;
                        }
                        return Ok(TaskAccess {
                            owner: actor.owner().clone(),
                            deadline: control
                                .valid_until()
                                .min(started + std::time::Duration::from_secs(5)),
                        });
                    }
                    Err(ComputerError::NotFound) => {}
                    Err(cause) => return Err(error(cause)),
                }
                let action = if cancel {
                    CommandTaskAction::Cancel
                } else {
                    CommandTaskAction::Observe
                };
                let command_access = self
                    .app
                    .store
                    .authorize_command_task(actor, id, action)
                    .await;
                let access = match command_access {
                    Ok(access) => access,
                    Err(ComputerError::NotFound) => {
                        use veoveo_computers::files::FileTaskAction;
                        let action = if cancel {
                            FileTaskAction::Cancel
                        } else {
                            FileTaskAction::Observe
                        };
                        let access = self
                            .app
                            .store
                            .authorize_file_task(actor, id, action)
                            .await
                            .map_err(error)?;
                        return Ok(TaskAccess {
                            owner: access.owner().map_err(error)?.clone(),
                            deadline: access.valid_until(),
                        });
                    }
                    Err(cause) => return Err(error(cause)),
                };
                Ok(TaskAccess {
                    owner: access.owner().map_err(error)?.clone(),
                    deadline: access.valid_until(),
                })
            }
            Err(cause) => Err(error(cause)),
        }
    }
}
