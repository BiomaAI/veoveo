use super::{ComputersMcp, auth};
use rmcp::{ErrorData, RoleServer, model::*, service::RequestContext};
use serde::Serialize;
use std::sync::LazyLock;
use uuid::Uuid;
use veoveo_mcp_contract::docs::ServerDocs;

pub const COLLECTION: &str = veoveo_computers::api::COMPUTERS_URI;
pub const COMPUTER_TEMPLATE: &str = "computer://computers/{computer_id}";
pub const ACCESS_TEMPLATE: &str = "computer://computers/{computer_id}/access";
pub const MAINTENANCE_TEMPLATE: &str = "computer://computers/{computer_id}/maintenance";
pub const EXECUTION_TEMPLATE: &str = "computer://executions/{execution_id}";
pub const TRANSFER_TEMPLATE: &str = "computer://transfers/{transfer_id}";
pub const AUTOMATION_TEMPLATE: &str = "computer://computers/{computer_id}/automation";
pub const GRANT_TEMPLATE: &str = "computer://computers/{computer_id}/automation/{grant_id}";
pub const PAGE_TEMPLATE: &str = "computer://computers?after={after}";
pub const DOC_TEMPLATE: &str = "computer://docs/{doc_id}";
pub static DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("computers"));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceId<'a> {
    Collection(Option<Uuid>),
    Computer(Uuid),
    Access(Uuid),
    Maintenance(Uuid),
    Execution(Uuid),
    Transfer(Uuid),
    Automation(Uuid),
    Grant(Uuid, Uuid),
    Docs,
    Doc(&'a str),
    Contract,
}
pub fn parse(uri: &str) -> Option<ResourceId<'_>> {
    match uri {
        COLLECTION => Some(ResourceId::Collection(None)),
        "computer://docs" => Some(ResourceId::Docs),
        "computer://contract" => Some(ResourceId::Contract),
        _ => {
            if uri.starts_with("computer://transfers/") {
                return veoveo_computers::api::FileTransferResultUri::try_from(uri.to_owned())
                    .ok()
                    .map(|uri| ResourceId::Transfer(uri.transfer_id()));
            }
            if uri.starts_with("computer://executions/") {
                return veoveo_computers::api::ExecutionResultUri::try_from(uri.to_owned())
                    .ok()
                    .map(|uri| ResourceId::Execution(uri.execution_id()));
            }
            if let Some(id) = uri.strip_prefix("computer://computers/") {
                if let Some(id) = id.strip_suffix("/maintenance") {
                    return canonical_uuid(id).map(ResourceId::Maintenance);
                }
                if let Some((computer, grant)) = id.split_once("/automation/") {
                    return Some(ResourceId::Grant(
                        canonical_uuid(computer)?,
                        canonical_uuid(grant)?,
                    ));
                }
                if let Some(id) = id.strip_suffix("/automation") {
                    return canonical_uuid(id).map(ResourceId::Automation);
                }
                if let Some(id) = id.strip_suffix("/access") {
                    return canonical_uuid(id).map(ResourceId::Access);
                }
                return canonical_uuid(id).map(ResourceId::Computer);
            }
            if let Some(id) = uri.strip_prefix("computer://computers?after=") {
                return canonical_uuid(id).map(|id| ResourceId::Collection(Some(id)));
            }
            uri.strip_prefix("computer://docs/")
                .filter(|id| DOCS.doc(id).is_some())
                .map(ResourceId::Doc)
        }
    }
}
pub fn canonical_uuid(text: &str) -> Option<Uuid> {
    let id = Uuid::parse_str(text).ok()?;
    (!id.is_nil() && id.to_string() == text).then_some(id)
}
pub fn roots() -> Vec<Resource> {
    [
        (
            COLLECTION,
            "computers",
            "Your Computers and current permitted actions",
        ),
        (
            "computer://docs",
            "docs",
            "Computer capability documentation",
        ),
        (
            "computer://contract",
            "contract",
            "Implemented server contract",
        ),
    ]
    .into_iter()
    .map(|(uri, name, description)| {
        Resource::new(uri, name)
            .with_description(description)
            .with_mime_type("application/json")
    })
    .collect()
}
pub fn templates() -> Vec<ResourceTemplate> {
    [
        (COMPUTER_TEMPLATE, "computer", "One private Computer"),
        (
            TRANSFER_TEMPLATE,
            "file-transfer-result",
            "Verified file transfer and governed Artifact reference",
        ),
        (
            MAINTENANCE_TEMPLATE,
            "computer-maintenance",
            "Admitted environment updates and active progress",
        ),
        (
            AUTOMATION_TEMPLATE,
            "computer-automation",
            "Live named automation grants for your Computer",
        ),
        (
            GRANT_TEMPLATE,
            "automation-grant",
            "Exact automation grant, including revoked or expired state",
        ),
        (
            EXECUTION_TEMPLATE,
            "execution-result",
            "Known command exit and governed output references",
        ),
        (
            ACCESS_TEMPLATE,
            "computer-access",
            "Outstanding access grants for your Computer",
        ),
        (
            PAGE_TEMPLATE,
            "computer-page",
            "Next page of your Computers",
        ),
        (DOC_TEMPLATE, "document", "Embedded server documentation"),
    ]
    .into_iter()
    .map(|(uri, name, description)| ResourceTemplate::new(uri, name).with_description(description))
    .collect()
}
pub fn json<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, ErrorData> {
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(
            serde_json::to_string(value).map_err(|_| auth::unavailable())?,
            uri,
        )
        .with_mime_type("application/json"),
    ]))
}
impl ComputersMcp {
    pub(super) async fn resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let actor = auth::actor(&context)?;
        if request.request_state.is_some() || request.input_responses.is_some() {
            return Err(ErrorData::invalid_params(
                "Computer resources have no interactive request state",
                None,
            ));
        }
        let uri = &request.uri;
        let result = match parse(uri)
            .ok_or_else(|| ErrorData::invalid_params("unknown Computer resource", None))?
        {
            ResourceId::Maintenance(id) => json(
                uri,
                &self
                    .app
                    .maintenance_state(&actor, id)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ResourceId::Collection(after) => json(
                uri,
                &self
                    .app
                    .snapshot(&actor, after)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ResourceId::Computer(id) => json(
                uri,
                &self
                    .app
                    .computer(&actor, id)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ResourceId::Access(id) => json(
                uri,
                &self
                    .app
                    .access_grants(&actor, id)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ResourceId::Execution(id) => {
                let access = self
                    .app
                    .store
                    .authorize_command_task(
                        &actor,
                        id,
                        veoveo_computers::commands::CommandTaskAction::Observe,
                    )
                    .await
                    .map_err(|error| super::read_error(error.into()))?;
                let read = async {
                    let task = veoveo_task_runtime::authorized_snapshot(
                        &self.app.tasks,
                        access.owner().map_err(|_| auth::forbidden())?,
                        &id.to_string(),
                    )
                    .await?;
                    if task.status != veoveo_task_runtime::TaskStatus::Succeeded
                        || task.task_type != "computer.execution"
                    {
                        return Err(ErrorData::invalid_params(
                            "command result is not available",
                            None,
                        ));
                    }
                    let response: CallToolResult =
                        serde_json::from_value(task.result.ok_or_else(auth::unavailable)?)
                            .map_err(|_| auth::unavailable())?;
                    let result: veoveo_computers::api::ExecutionResult = serde_json::from_value(
                        response.structured_content.ok_or_else(auth::unavailable)?,
                    )
                    .map_err(|_| auth::unavailable())?;
                    if result.execution_id != id
                        || result.computer_id != access.computer_id()
                        || String::from(result.result_uri) != *uri
                    {
                        return Err(auth::unavailable());
                    }
                    access.owner().map_err(|_| auth::forbidden())?;
                    json(uri, &result)
                };
                tokio::time::timeout_at(tokio::time::Instant::from_std(access.valid_until()), read)
                    .await
                    .map_err(|_| auth::forbidden())??
            }
            ResourceId::Transfer(id) => json(
                uri,
                &self
                    .app
                    .file_result(&actor, id)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ResourceId::Automation(id) => json(
                uri,
                &self
                    .app
                    .automation_grants(&actor, id)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ResourceId::Grant(computer, grant) => json(
                uri,
                &self
                    .app
                    .automation_grant(&actor, computer, grant)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ResourceId::Docs => json(uri, &DOCS.iter().collect::<Vec<_>>())?,
            ResourceId::Contract => json(uri, DOCS.contract_declaration())?,
            ResourceId::Doc(id) => {
                let doc = DOCS
                    .doc(id)
                    .ok_or_else(|| ErrorData::invalid_params("unknown document", None))?;
                ReadResourceResult::new(vec![
                    ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                ])
            }
        };
        Ok(veoveo_mcp_contract::private_resource_response(result, true))
    }
}
