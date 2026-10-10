//! One externally applied Reason process crash under the original public Task wait.
use super::*;
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::{sync::Mutex, time::Instant};
use veoveo_gateway_contract::ProtectedResourceId;
use veoveo_testing_support::{
    final_tasks::{
        WorkingCheckpoint, WorkingTaskRecovery,
        public_caller::{PrivateCallerJournal, read_private_input},
    },
    installed::restart::{CrashIdentity, CrashReceipt, CrashTarget, CrashWatch, DeploymentRestart},
    lifecycle::owner::{self, CleanupKind, CleanupRegistration},
};
use veoveo_types::ResourceAddress;

#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Schema {
    #[vocabulary(rename = "veoveo.ai/reason-process-crash/v1")]
    V1,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Fixture {
    schema: Schema,
    context: String,
    namespace: String,
    operator_resource: ProtectedResourceId,
    target: CrashTarget,
}
impl Fixture {
    fn admit(
        &self,
        installation: &veoveo_deploy_contract::InstallationTarget,
        resource: &ProtectedResourceId,
        public: bool,
        candidate: bool,
    ) -> Result<()> {
        self.target.validate()?;
        ensure!(
            public && !candidate,
            "Reason process recovery requires the normal public OAuth profile"
        );
        ensure!(
            self.schema == Schema::V1
                && self.context == installation.kubernetes.context
                && self.namespace == installation.kubernetes.namespace
                && self.operator_resource == *resource,
            "Reason crash fixture belongs to another installation/caller route"
        );
        ensure!(
            self.target.container == "reason-mcp"
                && installation
                    .expected_deployments
                    .contains(&self.target.deployment),
            "Reason crash must select the declared Rust server container"
        );
        Ok(())
    }
}
#[derive(Serialize)]
#[serde(
    tag = "observation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum Observation<'a> {
    Prepared {
        schema: &'static str,
        fixture: &'a Fixture,
    },
    TaskAcknowledged {
        id: &'a veoveo_types::CanonicalTaskId,
        task: &'a rmcp::model::Task,
    },
    Working {
        id: &'a veoveo_types::CanonicalTaskId,
        created: &'a rmcp::model::Task,
        current: &'a rmcp::model::DetailedTask,
    },
    ReplacementWorking {
        id: &'a veoveo_types::CanonicalTaskId,
        created: &'a rmcp::model::Task,
        current: &'a rmcp::model::DetailedTask,
    },
    Completed {
        id: &'a veoveo_types::CanonicalTaskId,
        statuses: &'a [rmcp::model::TaskStatus],
        result: &'a rmcp::model::CallToolResult,
    },
    WatchArmed {
        identity: &'a CrashIdentity,
    },
    ReadyForCrash {
        identity: &'a CrashIdentity,
    },
    Replacement {
        receipt: &'a CrashReceipt,
    },
    Cleanup {
        receipt: Option<&'a CrashReceipt>,
        closed: bool,
        persistence_failed: bool,
    },
}
// These facts outlive the cancellable SDK waiter and a failed journal append.
enum TaskFact {
    Acknowledged {
        id: veoveo_types::CanonicalTaskId,
        task: rmcp::model::Task,
    },
    Working {
        id: veoveo_types::CanonicalTaskId,
        created: rmcp::model::Task,
        current: rmcp::model::DetailedTask,
    },
    ReplacementWorking {
        id: veoveo_types::CanonicalTaskId,
        created: rmcp::model::Task,
        current: rmcp::model::DetailedTask,
    },
    Completed {
        id: veoveo_types::CanonicalTaskId,
        statuses: Vec<rmcp::model::TaskStatus>,
        result: rmcp::model::CallToolResult,
    },
}
impl TaskFact {
    fn observation(&self) -> Observation<'_> {
        match self {
            Self::Acknowledged { id, task } => Observation::TaskAcknowledged { id, task },
            Self::Working {
                id,
                created,
                current,
            } => Observation::Working {
                id,
                created,
                current,
            },
            Self::ReplacementWorking {
                id,
                created,
                current,
            } => Observation::ReplacementWorking {
                id,
                created,
                current,
            },
            Self::Completed {
                id,
                statuses,
                result,
            } => Observation::Completed {
                id,
                statuses,
                result,
            },
        }
    }
}
struct RetainedFact {
    value: TaskFact,
    flushed: bool,
}
type Closing = Pin<Box<dyn Future<Output = bool> + Send>>;
struct Handles {
    task_facts: Vec<RetainedFact>,
    watch: Option<CrashWatch>,
    closing: Option<Closing>,
    receipt: Option<CrashReceipt>,
    cleanup_cap: Option<Instant>,
    operation_end: Option<Instant>,
    failed: bool,
    persistence_failed: bool,
    closed: bool,
    journal: Arc<PrivateCallerJournal>,
}
impl Handles {
    fn flush_task_facts(&mut self) -> Result<()> {
        for fact in &mut self.task_facts {
            if !fact.flushed {
                let result = self.journal.append(&fact.value.observation());
                self.persistence_failed |= result.is_err();
                result?;
                fact.flushed = true;
            }
        }
        Ok(())
    }
    fn sync(&mut self, observation: &Observation<'_>) -> Result<()> {
        let retained = match observation {
            Observation::TaskAcknowledged { id, task } => Some(TaskFact::Acknowledged {
                id: (*id).clone(),
                task: (*task).clone(),
            }),
            Observation::Working {
                id,
                created,
                current,
            } => Some(TaskFact::Working {
                id: (*id).clone(),
                created: (*created).clone(),
                current: (*current).clone(),
            }),
            Observation::ReplacementWorking {
                id,
                created,
                current,
            } => Some(TaskFact::ReplacementWorking {
                id: (*id).clone(),
                created: (*created).clone(),
                current: (*current).clone(),
            }),
            Observation::Completed {
                id,
                statuses,
                result,
            } => Some(TaskFact::Completed {
                id: (*id).clone(),
                statuses: statuses.to_vec(),
                result: (*result).clone(),
            }),
            _ => None,
        };
        if let Some(value) = retained {
            ensure!(
                self.task_facts.len() < 5,
                "Reason retained Task observation budget exhausted"
            );
            self.task_facts.push(RetainedFact {
                value,
                flushed: false,
            });
            return self.flush_task_facts();
        }
        let result = self.journal.append(observation);
        self.persistence_failed |= result.is_err();
        result
    }
    async fn close(&mut self, deadline: Option<Instant>) -> Result<()> {
        if self.cleanup_cap.is_none() {
            let owner_end = Instant::from_std(owner::cleanup_deadline()?);
            self.cleanup_cap = Some(
                deadline
                    .or(self.operation_end)
                    .map_or(owner_end, |d| d.min(owner_end)),
            );
        }
        // Persistence failure never prevents polling the original native close.
        let _ = self.flush_task_facts();
        if self.closed {
            ensure!(
                !self.failed && !self.persistence_failed,
                "Reason crash cleanup remains unqualified"
            );
            return Ok(());
        }
        if let Some(watch) = &self.watch {
            self.receipt = watch.snapshot();
        }
        if self.closing.is_none()
            && let Some(watch) = self.watch.take()
        {
            self.closing = Some(Box::pin(watch.close()));
        }
        if let Some(closing) = self.closing.as_mut() {
            let cap = self
                .cleanup_cap
                .context("Reason crash cleanup cap missing")?;
            if self.failed || Instant::now() >= cap {
                self.failed = true;
            } else {
                match tokio::time::timeout_at(cap, closing).await {
                    Ok(true) => {
                        self.closing = None;
                        self.closed = true;
                    }
                    _ => self.failed = true,
                }
            }
        } else {
            self.closed = true;
        }
        self.persistence_failed |= self
            .journal
            .append(&Observation::Cleanup {
                receipt: self.receipt.as_ref(),
                closed: self.closed && !self.failed,
                persistence_failed: self.persistence_failed,
            })
            .is_err();
        ensure!(
            self.closed && !self.failed && !self.persistence_failed,
            "Reason crash watch cleanup/persistence failed; retained outcome unqualified"
        );
        Ok(())
    }
}
pub(super) struct Recovery {
    fixture: Fixture,
    installation: veoveo_deploy_contract::InstallationTarget,
    handles: Arc<Mutex<Handles>>,
    registration: CleanupRegistration,
    deadline: Option<Instant>,
}
impl Recovery {
    pub(super) fn load(
        path: &Path,
        installation: &InstalledTarget,
        public: Option<&public::Profile>,
        candidate: bool,
    ) -> Result<Self> {
        let fixture: Fixture = read_private_input(path)?;
        fixture.admit(
            &installation.target,
            &installation.operator.resource,
            public.is_some(),
            candidate,
        )?;
        let journal = public
            .context("Reason process recovery needs public OAuth")?
            .journal();
        journal.append(&Observation::Prepared {
            schema: "veoveo.ai/reason-process-recovery/v1",
            fixture: &fixture,
        })?;
        let handles = Arc::new(Mutex::new(Handles {
            task_facts: Vec::new(),
            watch: None,
            closing: None,
            receipt: None,
            cleanup_cap: None,
            operation_end: None,
            failed: false,
            persistence_failed: false,
            closed: false,
            journal,
        }));
        let retained = handles.clone();
        let registration = owner::register_cleanup(
            CleanupKind::Remote,
            "Reason crash watch",
            &fixture.target.pod_uid.to_string(),
            move || async move { retained.lock().await.close(None).await },
        )?;
        Ok(Self {
            fixture,
            installation: installation.target.clone(),
            handles,
            registration,
            deadline: None,
        })
    }
    pub(super) fn acknowledger(&self) -> Acknowledgments {
        Acknowledgments {
            handles: self.handles.clone(),
        }
    }
    pub(super) fn start(&mut self) -> Result<Instant> {
        let end = *self
            .deadline
            .get_or_insert_with(|| Instant::now() + Duration::from_secs(600));
        self.handles
            .try_lock()
            .map_err(|_| anyhow::anyhow!("Reason recovery observation ownership unavailable"))?
            .operation_end = Some(end);
        Ok(end)
    }
    pub(super) fn completed(
        &self,
        output: &veoveo_testing_support::final_tasks::DeliveredTaskResult,
    ) -> Result<()> {
        self.handles
            .try_lock()
            .map_err(|_| anyhow::anyhow!("Reason completion observation ownership unavailable"))?
            .sync(&Observation::Completed {
                id: &output.task_id,
                statuses: &output.statuses,
                result: &output.result,
            })
    }
    pub(super) async fn verify_current(&mut self) -> Result<()> {
        let mut owned = self.handles.lock().await;
        owned
            .watch
            .as_mut()
            .context("Reason replacement watch unavailable")?
            .admit_recovered_target()
            .await?;
        owned.receipt = owned.watch.as_ref().and_then(CrashWatch::snapshot);
        let receipt = owned
            .receipt
            .clone()
            .context("Reason replacement receipt missing")?;
        owned.sync(&Observation::Replacement { receipt: &receipt })
    }
    pub(super) async fn finish(&mut self) -> Result<()> {
        self.handles.lock().await.close(self.deadline).await?;
        self.registration.settled()?;
        Ok(())
    }
}
impl WorkingTaskRecovery for Recovery {
    fn replacement_working(
        &mut self,
        id: &veoveo_types::CanonicalTaskId,
        created: &rmcp::model::Task,
        current: &rmcp::model::DetailedTask,
    ) -> Result<()> {
        let mut owned = self
            .handles
            .try_lock()
            .map_err(|_| anyhow::anyhow!("Reason recovery observation ownership unavailable"))?;
        owned.sync(&Observation::ReplacementWorking {
            id,
            created,
            current,
        })
    }

    fn checkpoint<'a>(
        &'a mut self,
        context: WorkingCheckpoint<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let driver = DeploymentRestart::new(
                &self.installation,
                &self.fixture.target.deployment,
                "reason-mcp",
                context.client().peer().clone(),
                veoveo_reason_mcp::contract::ReasonResource::Contract.to_uri()?,
            )?;
            let mut owned = self.handles.lock().await;
            owned.sync(&Observation::Working {
                id: context.task_id(),
                created: context.created_task(),
                current: context.current_working(),
            })?;
            driver
                .arm_crash_watch(&self.fixture.target, &mut owned.watch)
                .await?;
            let identity = owned
                .watch
                .as_ref()
                .context("Reason crash watch was not retained")?
                .identity()?;
            owned.sync(&Observation::WatchArmed {
                identity: &identity,
            })?;
            // Re-admit after both Working observations immediately before permission.
            driver.admit_crash_target(&self.fixture.target).await?;
            let current = tokio::time::timeout_at(
                context.deadline(),
                context
                    .client()
                    .get_task(rmcp::model::GetTaskParams::new(context.task_id().as_str())),
            )
            .await
            .map_err(|_| anyhow::anyhow!("Reason fresh Working fence deadline"))?
            .map_err(|_| anyhow::anyhow!("Reason fresh Working fence read failed"))?;
            ready_after_fence(
                async { require_working(context.task_id(), context.created_task(), &current.task) },
                || {
                    owned.sync(&Observation::Working {
                        id: context.task_id(),
                        created: context.created_task(),
                        current: &current.task,
                    })?;
                    owned.sync(&Observation::ReadyForCrash {
                        identity: &identity,
                    })
                },
            )
            .await?;
            let cap = context
                .deadline()
                .min(Instant::now() + Duration::from_secs(300));
            let receipt = owned
                .watch
                .as_mut()
                .context("Reason crash watch unavailable")?
                .wait(cap)
                .await?;
            owned.receipt = Some(receipt.clone());
            owned.sync(&Observation::Replacement { receipt: &receipt })?;
            Ok(())
        })
    }
}

pub(super) struct Acknowledgments {
    handles: Arc<Mutex<Handles>>,
}
impl Acknowledgments {
    pub(super) fn record(
        &self,
        id: &veoveo_types::CanonicalTaskId,
        task: &rmcp::model::Task,
    ) -> Result<()> {
        self.handles
            .try_lock()
            .map_err(|_| anyhow::anyhow!("Reason acknowledgment ownership unavailable"))?
            .sync(&Observation::TaskAcknowledged { id, task })
    }
}
fn require_working(
    id: &veoveo_types::CanonicalTaskId,
    created: &rmcp::model::Task,
    current: &rmcp::model::DetailedTask,
) -> Result<()> {
    ensure!(
        veoveo_types::CanonicalTaskId::parse(&current.task.task_id)
            .ok()
            .as_ref()
            == Some(id)
            && current.task.created_at == created.created_at
            && matches!(current.payload, rmcp::model::TaskPayload::Working),
        "Reason crash requires original unfinished Working Task at the fresh fence"
    );
    Ok(())
}
async fn ready_after_fence(
    admission: impl Future<Output = Result<()>>,
    permission: impl FnOnce() -> Result<()>,
) -> Result<()> {
    admission.await?;
    permission()
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
