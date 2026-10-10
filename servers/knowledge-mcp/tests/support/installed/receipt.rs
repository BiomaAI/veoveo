//! Private outcome and request journal, created before any network operation.
use super::*;
use serde::Serialize;
use std::{
    io::{Seek, SeekFrom},
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Schema {
    #[vocabulary(rename = "veoveo.ai/knowledge-installed-outcome/v1")]
    V1,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum Outcome {
    Pending,
    Passed,
    Failed,
    Interrupted,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum Close {
    Absent,
    Open,
    Pending,
    Closed,
    Failed,
}
#[derive(Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub(super) enum Request {
    Connect,
    ToolsList,
    ResourcesList,
    TemplatesList,
    Read {
        uri: ResourceUri,
    },
    Tool {
        name: veoveo_gateway_contract::GatewayToolName,
    },
    Completion {
        template: veoveo_types::ResourceTemplateUri,
        argument: String,
        value: String,
    },
    Listen,
    Notification,
}
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Status {
    Pending,
    Complete,
    Mcp { code: i32 },
    Transport,
    Interrupted,
}
#[derive(Serialize)]
struct Observation {
    request: Request,
    #[serde(flatten)]
    status: Status,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PolicySnapshot {
    expected: policy::Expected,
    tools: Option<std::collections::BTreeSet<veoveo_gateway_contract::GatewayToolName>>,
    resources: Option<std::collections::BTreeSet<ResourceUri>>,
    templates: Option<std::collections::BTreeSet<veoveo_types::ResourceTemplateUri>>,
    collections: Option<std::collections::BTreeSet<CollectionId>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema: Schema,
    profile: GatewayProfileId,
    outcome: Outcome,
    operation: Outcome,
    requests: Vec<Observation>,
    caller: Close,
    listener: Close,
    native_observers: Close,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline: Option<InstalledReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<PolicySnapshot>,
    failure_sha256: Option<veoveo_types::Sha256Digest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cold_start: Option<cold_start::Observation>,
}
struct State {
    file: fs::File,
    report: Report,
}
#[derive(Clone)]
pub(super) struct Journal(Arc<Mutex<State>>);
impl Journal {
    pub fn open(path: &Path, profile: GatewayProfileId) -> Result<Self> {
        ensure!(
            path.is_absolute(),
            "Knowledge outcome requires absolute output path"
        );
        use std::os::unix::fs::OpenOptionsExt;
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|_| anyhow::anyhow!("Knowledge outcome requires new private file"))?;
        let journal = Self(Arc::new(Mutex::new(State {
            file,
            report: Report {
                schema: Schema::V1,
                profile,
                outcome: Outcome::Pending,
                operation: Outcome::Pending,
                requests: vec![],
                caller: Close::Absent,
                listener: Close::Absent,
                native_observers: Close::Absent,
                baseline: None,
                policy: None,
                failure_sha256: None,
                cold_start: None,
            },
        })));
        journal.update(|_| ())?;
        Ok(journal)
    }
    fn update(&self, edit: impl FnOnce(&mut Report)) -> Result<()> {
        let mut state = self.0.lock().expect("Knowledge outcome lock");
        edit(&mut state.report);
        let bytes = serde_json::to_vec_pretty(&state.report)?;
        state.file.seek(SeekFrom::Start(0))?;
        state.file.set_len(0)?;
        state.file.write_all(&bytes)?;
        state.file.write_all(b"\n")?;
        state.file.sync_all()?;
        Ok(())
    }
    pub fn native_observers(&self, state: Close) -> Result<()> {
        self.update(|r| {
            r.native_observers = state;
            if state == Close::Failed {
                r.outcome = Outcome::Failed;
            }
        })
    }
    pub fn cold_start(&self, value: cold_start::Observation) -> Result<()> {
        self.update(|r| r.cold_start = Some(value))
    }
    #[cfg(test)]
    pub fn break_writer_for_control(&self) -> Result<()> {
        self.0.lock().expect("Knowledge outcome lock").file =
            fs::OpenOptions::new().write(true).open("/dev/full")?;
        Ok(())
    }
    pub fn close(&self, caller: Close, listener: Close) -> Result<()> {
        self.update(|r| {
            r.caller = caller;
            r.listener = listener;
        })
    }
    pub fn policy(&self, expected: policy::Expected) -> Result<()> {
        self.update(|r| {
            r.policy = Some(PolicySnapshot {
                expected,
                tools: None,
                resources: None,
                templates: None,
                collections: None,
            })
        })
    }
    pub fn tools(
        &self,
        values: std::collections::BTreeSet<veoveo_gateway_contract::GatewayToolName>,
    ) -> Result<()> {
        self.update(|r| r.policy.as_mut().expect("policy").tools = Some(values))
    }
    pub fn resources(&self, values: std::collections::BTreeSet<ResourceUri>) -> Result<()> {
        self.update(|r| r.policy.as_mut().expect("policy").resources = Some(values))
    }
    pub fn templates(
        &self,
        values: std::collections::BTreeSet<veoveo_types::ResourceTemplateUri>,
    ) -> Result<()> {
        self.update(|r| r.policy.as_mut().expect("policy").templates = Some(values))
    }
    pub fn collections(&self, values: std::collections::BTreeSet<CollectionId>) -> Result<()> {
        self.update(|r| r.policy.as_mut().expect("policy").collections = Some(values))
    }
    pub fn failure(&self, error: &anyhow::Error) -> Result<()> {
        use sha2::{Digest, Sha256};
        self.update(|r| {
            r.failure_sha256 = Some(veoveo_types::Sha256Digest::from_bytes(
                Sha256::digest(error.to_string().as_bytes()).into(),
            ))
        })
    }
    pub fn operation<T>(&self, result: &Result<T>) -> Result<()> {
        self.update(|r| {
            r.operation = if result.is_ok() {
                Outcome::Passed
            } else {
                Outcome::Failed
            }
        })?;
        if let Err(error) = result {
            self.failure(error)?;
        }
        Ok(())
    }
    pub fn finish(&self, ok: bool, baseline: Option<InstalledReport>) -> Result<()> {
        self.update(|r| {
            if ok {
                r.outcome = Outcome::Passed;
            } else if r.outcome != Outcome::Interrupted {
                r.outcome = Outcome::Failed;
            }
            r.baseline = baseline;
        })
    }
    pub fn cleanup(&self, caller: Close, listener: Close) -> Result<()> {
        self.update(|r| {
            r.caller = caller;
            r.listener = listener;
            if r.operation == Outcome::Pending {
                r.operation = Outcome::Interrupted;
            }
            if r.outcome == Outcome::Pending {
                r.outcome = Outcome::Interrupted;
            }
            if !matches!(caller, Close::Absent | Close::Closed)
                || !matches!(listener, Close::Absent | Close::Closed)
            {
                r.outcome = Outcome::Failed;
            }
            for observation in &mut r.requests {
                if matches!(observation.status, Status::Pending) {
                    observation.status = Status::Interrupted;
                }
            }
        })
    }
    async fn acquire_slot<T, E>(
        &self,
        request: Request,
        slot: &mut Option<T>,
        future: impl std::future::Future<Output = std::result::Result<T, E>>,
    ) -> Result<()> {
        let index = {
            let state = self.0.lock().expect("Knowledge outcome lock");
            ensure!(
                state.report.requests.len() < 512,
                "Knowledge request budget exhausted"
            );
            state.report.requests.len()
        };
        self.update(|r| {
            r.requests.push(Observation {
                request,
                status: Status::Pending,
            })
        })?;
        let result = future.await;
        let ok = result.is_ok();
        // The actual handle reaches retained storage before any fallible journal
        // write; a disk failure must not discard an acknowledged SDK handle.
        if let Ok(handle) = result {
            *slot = Some(handle);
        }
        self.update(|r| {
            r.requests[index].status = if ok {
                Status::Complete
            } else {
                Status::Transport
            }
        })?;
        ensure!(
            ok,
            "Knowledge SDK acquisition failed; inspect private outcome"
        );
        Ok(())
    }
    pub async fn acquire(
        &self,
        slot: &mut Option<rmcp::service::RunningService<RoleClient, ClientConfig>>,
        future: impl std::future::Future<
            Output = Result<rmcp::service::RunningService<RoleClient, ClientConfig>>,
        >,
    ) -> Result<()> {
        self.acquire_slot(Request::Connect, slot, future).await
    }
    pub async fn listen(
        &self,
        slot: &mut Option<rmcp::service::Subscription>,
        peer: &Peer<RoleClient>,
        filter: SubscriptionFilter,
    ) -> Result<()> {
        self.acquire_slot(Request::Listen, slot, peer.listen(filter))
            .await
    }
    pub async fn request<T>(
        &self,
        request: Request,
        future: impl std::future::Future<Output = std::result::Result<T, rmcp::ServiceError>>,
    ) -> Result<T> {
        let index = {
            let state = self.0.lock().expect("Knowledge outcome lock");
            ensure!(
                state.report.requests.len() < 512,
                "Knowledge request budget exhausted"
            );
            state.report.requests.len()
        };
        self.update(|r| {
            r.requests.push(Observation {
                request,
                status: Status::Pending,
            })
        })?;
        let result = future.await;
        self.update(|r| {
            r.requests[index].status = match &result {
                Ok(_) => Status::Complete,
                Err(rmcp::ServiceError::McpError(e)) => Status::Mcp { code: e.code.0 },
                Err(_) => Status::Transport,
            }
        })?;
        result.map_err(|_| anyhow::anyhow!("Knowledge request failed; inspect private outcome"))
    }
    pub async fn denied(&self, peer: &Peer<RoleClient>, uri: ResourceUri) -> Result<()> {
        let index = {
            let state = self.0.lock().expect("Knowledge outcome lock");
            ensure!(
                state.report.requests.len() < 512,
                "Knowledge request budget exhausted"
            );
            state.report.requests.len()
        };
        self.update(|r| {
            r.requests.push(Observation {
                request: Request::Read { uri: uri.clone() },
                status: Status::Pending,
            })
        })?;
        let result = peer
            .read_resource(ReadResourceRequestParams::new(uri.as_str()))
            .await;
        self.update(|r| {
            r.requests[index].status = match &result {
                Ok(_) => Status::Complete,
                Err(rmcp::ServiceError::McpError(e)) => Status::Mcp { code: e.code.0 },
                Err(_) => Status::Transport,
            }
        })?;
        require_denied(result)
    }
}
pub(super) fn require_denied<T>(result: std::result::Result<T, rmcp::ServiceError>) -> Result<()> {
    ensure!(
        matches!(result,Err(rmcp::ServiceError::McpError(ref e)) if e.code==ErrorCode::INVALID_REQUEST),
        "Knowledge document requires exact policy denial -32600"
    );
    Ok(())
}
