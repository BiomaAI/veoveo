use super::{ComputersMcp, auth};
use rmcp::{ErrorData, RoleServer, model::*, service::RequestContext};
use serde::Serialize;
use std::sync::LazyLock;
use uuid::Uuid;
use veoveo_mcp_contract::docs::ServerDocs;

pub const COLLECTION: &str = veoveo_computers::api::COMPUTERS_URI;
pub const COMPUTER_TEMPLATE: &str = "computer://computers/{computer_id}";
pub const PAGE_TEMPLATE: &str = "computer://computers?after={after}";
pub const DOC_TEMPLATE: &str = "computer://docs/{doc_id}";
pub static DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("computers"));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceId<'a> {
    Collection(Option<Uuid>),
    Computer(Uuid),
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
            if let Some(id) = uri.strip_prefix("computer://computers/") {
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
