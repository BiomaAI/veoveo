//! Typed public mutations, contentless SSE invalidations and retained cleanup.
use super::*;
use futures::TryStreamExt;
use std::sync::atomic::{AtomicUsize, Ordering};

type Events =
    Pin<Box<dyn Stream<Item = std::result::Result<sse_stream::Sse, sse_stream::Error>> + Send>>;
#[derive(Serialize)]
#[serde(
    tag = "phase",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum Record<'a> {
    Prepared {
        schema: &'static str,
        input: &'a input::Input,
    },
    Intent {
        action: Action,
        request_id: Uuid,
        request: Mutation<'a>,
    },
    Definition {
        value: &'a wire::Definition,
    },
    Instance {
        value: &'a wire::ManagedInstance,
    },
    Operation {
        value: &'a wire::LifecycleOperation,
    },
    SseArmed {
        revision: &'a Sha256Digest,
    },
    Changed {
        revision: &'a Sha256Digest,
    },
    Uncertain {
        action: Action,
        request_id: Uuid,
    },
    Cleanup {
        passed: bool,
    },
    Outcome {
        operation_passed: bool,
        cleanup_passed: bool,
        physical_drain_proven: bool,
        retained_storage: bool,
    },
}
#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Action {
    Create,
    Publish,
    Provision,
    Stop,
    ArchiveInstance,
    ArchiveDefinition,
}
#[derive(Clone, Copy, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(super) enum Mutation<'a> {
    Create(&'a wire::CreateDefinition),
    Publish(&'a wire::PublishDefinition),
    Provision(&'a wire::ProvisionInstance),
    Update(&'a wire::UpdateInstance),
    Archive(&'a wire::RevisionRequest),
}
#[derive(Clone, Copy)]
struct Attempt {
    request_id: Uuid,
    generation: Option<i64>,
}
#[derive(Default)]
struct Dispatch {
    create: Option<Attempt>,
    publish: Option<Attempt>,
    provision: Option<Attempt>,
    stop: Option<Attempt>,
    archive_instance: Option<Attempt>,
    archive_definition: Option<Attempt>,
}
pub(super) struct Journey {
    pub input: input::Input,
    client: Option<reqwest::Client>,
    base: url::Url,
    journal: Arc<PrivateCallerJournal>,
    deadline: Instant,
    events: Option<Events>,
    changes: usize,
    dispatch: Dispatch,
    create_request: Option<wire::CreateDefinition>,
    provision_request: Option<wire::ProvisionInstance>,
    definition: Option<wire::Definition>,
    instance: Option<wire::ManagedInstance>,
    operation: Option<wire::LifecycleOperation>,
    expected_operation: Option<Uuid>,
    expected_generation: Option<i64>,
    published_revision: Option<Sha256Digest>,
    cleanup_cap: Option<Instant>,
    cleanup_done: bool,
    failed: bool,
}
impl Journey {
    pub fn new(
        input: input::Input,
        client: reqwest::Client,
        base: url::Url,
        journal: Arc<PrivateCallerJournal>,
        deadline: Instant,
    ) -> Self {
        Self {
            input,
            client: Some(client),
            base,
            journal,
            deadline,
            events: None,
            changes: 0,
            dispatch: Dispatch::default(),
            create_request: None,
            provision_request: None,
            definition: None,
            instance: None,
            operation: None,
            expected_operation: None,
            expected_generation: None,
            published_revision: None,
            cleanup_cap: None,
            cleanup_done: false,
            failed: false,
        }
    }
    fn url(&self, parts: &[&str]) -> Result<url::Url> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|_| anyhow!("invalid admin origin"))?
            .pop_if_empty()
            .extend(parts);
        Ok(url)
    }
    fn get<T: DeserializeOwned + Send + 'static>(
        &self,
        parts: &[&str],
    ) -> futures::future::BoxFuture<'static, Result<T>> {
        let request = (|| {
            Ok::<_, anyhow::Error>(
                self.client
                    .as_ref()
                    .context("HTTP client closed")?
                    .get(self.url(parts)?),
            )
        })();
        Box::pin(async move {
            let response = request?
                .send()
                .await
                .map_err(|_| anyhow!("public lifecycle read transport failed"))?;
            decode(response).await
        })
    }
    fn absent(&self, parts: &[&str]) -> futures::future::BoxFuture<'static, Result<()>> {
        let request = (|| {
            Ok::<_, anyhow::Error>(
                self.client
                    .as_ref()
                    .context("HTTP client closed")?
                    .get(self.url(parts)?),
            )
        })();
        Box::pin(async move {
            let response = request?
                .send()
                .await
                .map_err(|_| anyhow!("initial ownership read failed"))?;
            ensure!(
                response.status() == reqwest::StatusCode::NOT_FOUND,
                "selected identity already exists or absence is unqualified"
            );
            Ok(())
        })
    }
    fn intent(&mut self, action: Action, id: Uuid, request: Mutation<'_>) -> Result<()> {
        let generation = match &request {
            Mutation::Provision(_) => Some(1),
            Mutation::Update(r) => Some(r.expected_generation + 1),
            _ => None,
        };
        let slot = match action {
            Action::Create => &mut self.dispatch.create,
            Action::Publish => &mut self.dispatch.publish,
            Action::Provision => &mut self.dispatch.provision,
            Action::Stop => &mut self.dispatch.stop,
            Action::ArchiveInstance => &mut self.dispatch.archive_instance,
            Action::ArchiveDefinition => &mut self.dispatch.archive_definition,
        };
        ensure!(
            slot.is_none(),
            "mutation already dispatched; read reconciliation required"
        );
        self.journal.append(&Record::Intent {
            action,
            request_id: id,
            request,
        })?;
        match request {
            Mutation::Create(value) => self.create_request = Some(value.clone()),
            Mutation::Provision(value) => self.provision_request = Some(value.clone()),
            _ => {}
        }
        *slot = Some(Attempt {
            request_id: id,
            generation,
        });
        if let Some(generation) = generation {
            self.expected_generation = Some(generation);
            self.expected_operation = None;
        }
        Ok(())
    }
    fn mutate<T: DeserializeOwned + Send + 'static>(
        &self,
        method: reqwest::Method,
        parts: &[&str],
        request: &impl Serialize,
        action: Action,
        id: Uuid,
    ) -> futures::future::BoxFuture<'static, Result<T>> {
        let request = (|| {
            Ok::<_, anyhow::Error>(
                self.client
                    .as_ref()
                    .context("HTTP client closed")?
                    .request(method, self.url(parts)?)
                    .json(request),
            )
        })();
        let journal = self.journal.clone();
        Box::pin(async move {
            let result = async {
                let response = request?
                    .send()
                    .await
                    .map_err(|_| anyhow!("public lifecycle dispatch outcome unknown"))?;
                decode(response).await
            }
            .await;
            if result.is_err() {
                journal.append(&Record::Uncertain {
                    action,
                    request_id: id,
                })?;
            }
            result
        })
    }
    async fn arm(&mut self) -> Result<()> {
        let response = self
            .client
            .as_ref()
            .context("HTTP client closed")?
            .get(self.url(&["agent-events"])?)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .timeout(Duration::from_secs(self.input.timeout_seconds))
            .send()
            .await
            .map_err(|_| anyhow!("public SSE acquisition failed"))?;
        ensure!(
            response.status().is_success()
                && response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|s| s.split(';').next() == Some("text/event-stream")),
            "public SSE admission refused"
        );
        let count = Arc::new(AtomicUsize::new(0));
        let bounded = response
            .bytes_stream()
            .map_err(|_| std::io::Error::other("SSE transport failed"))
            .and_then(move |bytes| {
                let used = count.fetch_add(bytes.len(), Ordering::Relaxed) + bytes.len();
                futures::future::ready(if used <= 256 * 1024 {
                    Ok(bytes)
                } else {
                    Err(std::io::Error::other("SSE byte budget exceeded"))
                })
            });
        // Retain the exact response stream before polling the initial event.
        self.events = Some(Box::pin(sse_stream::SseStream::from_bytes_stream(bounded)));
        let revision = self.next_change().await?;
        self.journal.append(&Record::SseArmed {
            revision: &revision,
        })
    }
    async fn next_change(&mut self) -> Result<Sha256Digest> {
        loop {
            ensure!(self.changes < 128, "SSE observation budget exhausted");
            let event = tokio::time::timeout_at(
                self.deadline,
                self.events
                    .as_mut()
                    .context("SSE unavailable; outcome unresolved")?
                    .next(),
            )
            .await
            .map_err(|_| anyhow!("public SSE deadline expired"))?
            .context("public SSE ended; outcome unresolved")?
            .map_err(|_| anyhow!("public SSE decoding failed"))?;
            self.changes += 1;
            if let Some(revision) = admit_event(&event)? {
                self.journal.append(&Record::Changed {
                    revision: &revision,
                })?;
                return Ok(revision);
            }
        }
    }
    fn definition_observed(&mut self, value: wire::Definition) -> Result<()> {
        // Keep the received typed observation even if independent assertions/sync fail.
        self.definition = Some(value);
        let value = self.definition.as_ref().expect("received definition");
        self.journal.append(&Record::Definition { value })?;
        ensure!(
            value.id == self.input.definition
                && value.owner == self.input.owner
                && value.work_context == self.input.work_context,
            "definition owner/context/identity differs"
        );
        Ok(())
    }
    fn instance_observed(&mut self, value: wire::ManagedInstance) -> Result<()> {
        self.instance = Some(value);
        let value = self.instance.as_ref().expect("received instance");
        self.journal.append(&Record::Instance { value })?;
        admit_instance(&self.input, value)?;
        Ok(())
    }
    async fn refresh_definition(&mut self) -> Result<()> {
        let value = self
            .get(&["agent-definitions", self.input.definition.as_str()])
            .await?;
        self.definition_observed(value)
    }
    async fn refresh_instance(&mut self) -> Result<()> {
        let value = self
            .get(&["agent-instances", self.input.instance.as_str()])
            .await?;
        self.instance_observed(value)?;
        let instance = self.instance.as_ref().expect("observed instance");
        ensure!(
            self.expected_generation == Some(instance.generation)
                && self
                    .expected_operation
                    .is_none_or(|id| id == instance.operation)
                && self.published_revision.as_ref() == Some(&instance.requested_revision),
            "instance revision/generation/acknowledged operation differs"
        );
        self.expected_operation.get_or_insert(instance.operation);
        let operation: wire::LifecycleOperation = self
            .get(&["agent-operations", &instance.operation.to_string()])
            .await?;
        self.operation = Some(operation);
        let operation = self.operation.as_ref().expect("received operation");
        self.journal
            .append(&Record::Operation { value: operation })?;
        ensure!(
            operation.instance == instance.id
                && operation.generation == instance.generation
                && operation.id == instance.operation,
            "operation/instance/generation differs"
        );
        Ok(())
    }
    pub async fn execute(&mut self) -> Result<()> {
        self.absent(&["agent-definitions", self.input.definition.as_str()])
            .await?;
        self.absent(&["agent-instances", self.input.instance.as_str()])
            .await?;
        let authoring: wire::Authoring = self.get(&["agent-authoring"]).await?;
        ensure!(
            authoring.work_context == self.input.work_context
                && authoring.permissions.create
                && authoring.permissions.publish
                && authoring.permissions.archive
                && authoring.permissions.deploy
                && authoring.permissions.instance_control
                && authoring
                    .models
                    .iter()
                    .any(|m| m.reference == self.input.model),
            "current authoring/model authority refuses journey"
        );
        let templates: Vec<wire::TemplateChoice> = self.get(&["agent-templates"]).await?;
        ensure!(
            templates.len() <= 32
                && templates.iter().any(|t| t.id == self.input.template.id
                    && t.revision == self.input.template_revision()
                    && t.tools.is_empty()
                    && t.parameters.is_empty()
                    && t.resource_subscriptions.is_empty()),
            "current approved idle template differs"
        );
        self.arm().await?;
        let request_id = Uuid::now_v7();
        let content = wire::Content {
            model: self.input.model.clone(),
            instructions: "Idle lifecycle qualification; no episode requested".into(),
            tools: vec![],
            budgets: wire::Budgets {
                max_output_tokens: 64,
                max_completion_calls: 1,
                max_tool_calls: 0,
                deadline_seconds: 30,
            },
            execution: wire::Execution::Managed {
                template: self.input.template.id.clone(),
                template_revision: self.input.template_revision(),
                parameters: Default::default(),
                resource_subscriptions: vec![],
            },
        };
        let request = wire::CreateDefinition {
            request_id,
            id: self.input.definition.clone(),
            name: "Idle lifecycle qualification".into(),
            description: "Owned installed lifecycle fixture".into(),
            source: wire::DefinitionSource::Blank { content },
        };
        self.intent(Action::Create, request_id, Mutation::Create(&request))?;
        match self
            .mutate(
                reqwest::Method::POST,
                &["agent-definitions"],
                &request,
                Action::Create,
                request_id,
            )
            .await
        {
            Ok(value) => self.definition_observed(value)?,
            Err(_) => self.refresh_definition().await?,
        }
        let draft: wire::Draft = self
            .get(&["agent-definitions", self.input.definition.as_str(), "draft"])
            .await?;
        let wire::DefinitionSource::Blank { content: expected } = &request.source else {
            unreachable!()
        };
        ensure!(
            draft.definition == self.input.definition
                && draft.content == *expected
                && self
                    .definition
                    .as_ref()
                    .is_some_and(|d| d.revision == draft.revision),
            "created/reconciled draft content differs from retained request"
        );
        let draft = self.definition.as_ref().expect("created definition");
        let request = wire::PublishDefinition {
            request_id: Uuid::now_v7(),
            expected_revision: draft.revision,
            digest: draft.draft_digest.clone(),
            audience: vec![self.input.work_context.clone()],
        };
        let request_id = request.request_id;
        self.intent(Action::Publish, request_id, Mutation::Publish(&request))?;
        match self
            .mutate(
                reqwest::Method::POST,
                &[
                    "agent-definitions",
                    self.input.definition.as_str(),
                    "publish",
                ],
                &request,
                Action::Publish,
                request_id,
            )
            .await
        {
            Ok(value) => self.definition_observed(value)?,
            Err(_) => self.refresh_definition().await?,
        }
        ensure!(
            self.definition
                .as_ref()
                .is_some_and(|d| d.published_digest.as_ref() == Some(&request.digest)),
            "publication not established; intent retained"
        );
        self.published_revision = Some(request.digest.clone());
        let request_id = Uuid::now_v7();
        let request = wire::ProvisionInstance {
            request_id,
            id: self.input.instance.clone(),
            name: "Idle lifecycle qualification".into(),
            definition: self.input.definition.clone(),
            revision: request.digest,
        };
        self.intent(Action::Provision, request_id, Mutation::Provision(&request))?;
        if let Ok(operation) = self
            .mutate::<wire::LifecycleOperation>(
                reqwest::Method::POST,
                &["agent-instances"],
                &request,
                Action::Provision,
                request_id,
            )
            .await
        {
            self.expected_operation = Some(operation.id);
            self.operation = Some(operation);
            let operation = self.operation.as_ref().expect("received operation");
            self.journal
                .append(&Record::Operation { value: operation })?;
            ensure!(
                operation.instance == self.input.instance && operation.generation == 1,
                "provision acknowledgment differs"
            );
        }
        self.await_phase(wire::InstancePhase::Ready, 1).await
    }
    async fn await_phase(&mut self, phase: wire::InstancePhase, generation: i64) -> Result<()> {
        loop {
            // A current view alone cannot qualify actual post-dispatch delivery.
            self.next_change().await?;
            self.refresh_instance().await?;
            let instance = self.instance.as_ref().expect("instance");
            ensure!(
                instance.generation == generation,
                "instance generation changed outside retained request"
            );
            ensure!(
                !matches!(
                    instance.observed,
                    wire::InstancePhase::Failed | wire::InstancePhase::Superseded
                ),
                "Manager reported failed or superseded operation"
            );
            if instance.observed == phase {
                let operation = self.operation.as_ref().expect("operation");
                ensure!(
                    operation.phase == phase,
                    "current operation has not settled selected phase"
                );
                if phase == wire::InstancePhase::Ready {
                    ensure!(
                        instance.active_generation == generation
                            && instance.active_revision.as_ref()
                                == Some(&instance.requested_revision),
                        "Ready active generation/revision differs"
                    );
                }
                return Ok(());
            }
        }
    }
    async fn state_change(
        &mut self,
        action: Action,
        change: wire::InstanceChange,
        phase: wire::InstancePhase,
    ) -> Result<()> {
        self.refresh_instance().await?;
        let existing = match action {
            Action::Stop => self.dispatch.stop,
            Action::ArchiveInstance => self.dispatch.archive_instance,
            _ => None,
        };
        let generation = if let Some(attempt) = existing {
            let _retained_request_id = attempt.request_id;
            attempt
                .generation
                .context("retained mutation generation missing")?
        } else {
            let previous = self.instance.as_ref().expect("instance").generation;
            let request_id = Uuid::now_v7();
            let request = wire::UpdateInstance {
                request_id,
                expected_generation: previous,
                change,
            };
            self.intent(action, request_id, Mutation::Update(&request))?;
            if let Ok(operation) = self
                .mutate::<wire::LifecycleOperation>(
                    reqwest::Method::PATCH,
                    &["agent-instances", self.input.instance.as_str()],
                    &request,
                    action,
                    request_id,
                )
                .await
            {
                self.expected_operation = Some(operation.id);
                self.operation = Some(operation);
                let operation = self.operation.as_ref().expect("received operation");
                self.journal
                    .append(&Record::Operation { value: operation })?;
                ensure!(
                    operation.instance == self.input.instance
                        && operation.generation == previous + 1,
                    "lifecycle acknowledgment differs"
                );
            }
            previous + 1
        };
        if action == Action::Stop {
            // Stop fences dispatch through a new epoch. Archive may supersede its
            // queued operation; Stop does not mean Paused or physical shutdown.
            self.refresh_instance().await?;
            ensure!(
                self.instance
                    .as_ref()
                    .is_some_and(|i| i.generation == generation),
                "stop outcome unresolved"
            );
            Ok(())
        } else {
            self.await_phase(phase, generation).await
        }
    }
    async fn admit_owned_definition(&mut self) -> Result<()> {
        self.refresh_definition().await?;
        let request = self
            .create_request
            .as_ref()
            .context("original create request unavailable; cleanup unresolved")?;
        let value = self.definition.as_ref().expect("observed definition");
        ensure!(
            value.id == request.id
                && value.name == request.name
                && value.description == request.description,
            "definition differs from original create request; cleanup unresolved"
        );
        let draft: wire::Draft = self
            .get(&["agent-definitions", self.input.definition.as_str(), "draft"])
            .await?;
        let wire::DefinitionSource::Blank { content } = &request.source else {
            return Err(anyhow!(
                "unsupported original create source; cleanup unresolved"
            ));
        };
        ensure!(
            draft.definition == request.id
                && draft.revision == value.revision
                && draft.content == *content,
            "definition draft differs from original create request; cleanup unresolved"
        );
        ensure!(
            value.published_digest == self.published_revision,
            "definition publication differs from retained publication; cleanup unresolved"
        );
        Ok(())
    }
    async fn cleanup_remote(&mut self) -> Result<()> {
        // No destructive action follows a mere create intent or matching owner/context.
        if self.dispatch.create.is_some() || self.dispatch.provision.is_some() {
            self.admit_owned_definition().await?;
        }
        if self.dispatch.provision.is_some() {
            self.refresh_instance().await?;
            let request = self
                .provision_request
                .as_ref()
                .context("original provision request unavailable; cleanup unresolved")?;
            let value = self.instance.as_ref().expect("observed instance");
            ensure!(
                value.id == request.id
                    && value.definition == request.definition
                    && value.name == request.name
                    && value.requested_revision == request.revision
                    && value.storage_gib == self.input.template.workload.storage_gib,
                "instance differs from original provision request; cleanup unresolved"
            );
            if self
                .instance
                .as_ref()
                .is_some_and(|i| i.observed != wire::InstancePhase::Archived)
            {
                if self.dispatch.stop.is_none() {
                    self.state_change(
                        Action::Stop,
                        wire::InstanceChange::Stop,
                        wire::InstancePhase::Ready,
                    )
                    .await?;
                }
                self.state_change(
                    Action::ArchiveInstance,
                    wire::InstanceChange::State {
                        desired: wire::InstanceDesired::Archived,
                    },
                    wire::InstancePhase::Archived,
                )
                .await?;
            }
        }
        if self.dispatch.create.is_some() {
            self.admit_owned_definition().await?;
            if self
                .definition
                .as_ref()
                .is_some_and(|d| d.status != wire::DefinitionStatus::Archived)
            {
                ensure!(
                    self.dispatch.archive_definition.is_none(),
                    "definition archive dispatch unresolved"
                );
                let request_id = Uuid::now_v7();
                let request = wire::RevisionRequest {
                    request_id,
                    expected_revision: self.definition.as_ref().expect("definition").revision,
                };
                self.intent(
                    Action::ArchiveDefinition,
                    request_id,
                    Mutation::Archive(&request),
                )?;
                match self
                    .mutate(
                        reqwest::Method::POST,
                        &[
                            "agent-definitions",
                            self.input.definition.as_str(),
                            "archive",
                        ],
                        &request,
                        Action::ArchiveDefinition,
                        request_id,
                    )
                    .await
                {
                    Ok(value) => self.definition_observed(value)?,
                    Err(_) => self.refresh_definition().await?,
                }
                ensure!(
                    self.definition
                        .as_ref()
                        .is_some_and(|d| d.status == wire::DefinitionStatus::Archived),
                    "definition archive unresolved"
                );
            }
        }
        Ok(())
    }
    pub async fn finish(&mut self) -> Result<()> {
        if self.cleanup_done {
            ensure!(!self.failed, "prior cleanup failed; identities retained");
            return Ok(());
        }
        let cap = *self.cleanup_cap.get_or_insert(
            self.deadline
                .min(Instant::from_std(owner::cleanup_deadline()?)),
        );
        let result = if Instant::now() >= cap {
            Err(anyhow!("original journey cleanup deadline expired"))
        } else {
            tokio::time::timeout_at(cap, self.cleanup_remote())
                .await
                .map_err(|_| anyhow!("original journey cleanup deadline expired"))
                .and_then(|r| r)
        };
        // reqwest exposes cancellation by body drop, no asynchronous close API.
        self.events.take();
        self.client.take();
        self.failed |= result.is_err();
        let persisted = self.journal.append(&Record::Cleanup {
            passed: !self.failed,
        });
        self.failed |= persisted.is_err();
        self.cleanup_done = true;
        result?;
        persisted?;
        ensure!(!self.failed, "cleanup unqualified");
        Ok(())
    }
}
pub(super) fn admit_instance(input: &input::Input, value: &wire::ManagedInstance) -> Result<()> {
    ensure!(
        value.id == input.instance
            && value.definition == input.definition
            && value.owner == input.owner
            && value.work_context == input.work_context
            && value.template == input.template.id
            && value.generation > 0,
        "instance identity/owner/context/revision differs"
    );
    Ok(())
}
pub(super) fn admit_event(event: &sse_stream::Sse) -> Result<Option<Sha256Digest>> {
    match event.event.as_deref() {
        Some("expired") => Err(anyhow!("public SSE authority expired")),
        Some("change") => {
            let data = event.data.as_deref().context("SSE change data missing")?;
            ensure!(data.len() <= 4096, "SSE change exceeds bound");
            let wake: wire::CatalogWake =
                serde_json::from_str(data).map_err(|_| anyhow!("invalid typed SSE wake"))?;
            ensure!(
                event.id.as_deref() == Some(wake.revision.as_str()),
                "SSE revision/id differs"
            );
            Ok(Some(wake.revision))
        }
        None if event.data.is_none() => Ok(None),
        _ => Err(anyhow!("unsupported lifecycle SSE event")),
    }
}
async fn decode<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    ensure!(
        response.status().is_success(),
        "public lifecycle request refused"
    );
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| anyhow!("public lifecycle response transport failed"))?;
        ensure!(
            bytes.len() + chunk.len() <= 256 * 1024,
            "public lifecycle response exceeds 256 KiB"
        );
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| anyhow!("public lifecycle response shape refused"))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
