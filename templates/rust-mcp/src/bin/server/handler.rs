//! The MCP surface: tools, typed reads, prompts and completion.
//!
//! `GlossaryMcp` implements `DomainServer`. The shared host serves discovery,
//! documents, the contract, authentication and transport, and calls these
//! methods only for Glossary's own addresses, tools and prompts.

use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, GetPromptRequestParams,
        GetPromptResult, Prompt, PromptArgument, PromptMessage, ReadResourceRequestParams,
        Reference, Role,
    },
    service::RequestContext,
    tool, tool_router,
};
use serde::Deserialize;
use veoveo_mcp_contract::{
    hosting::{
        DomainAddress, DomainRead, DomainServer, completion, gateway_identity, json_read,
        rank_completions, served_by_host, structured_result, unknown_prompt,
    },
    server_contract::McpServerSetup,
};
use veoveo_types::ResourceAddress;

use veoveo_glossary_mcp::{
    contract::{DefineRequest, Definition, GlossaryResource, TERM_TEMPLATE, TermId, TermSummary},
    glossary,
};

use crate::setup::{GlossaryContract, SERVER_SETUP};

const EXPLAIN_PROMPT: &str = "explain_term";

#[derive(Clone)]
pub struct GlossaryMcp {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl GlossaryMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Define term",
        description = "Return the definition of one glossary term and its related terms.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<Definition>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn define(
        &self,
        Parameters(request): Parameters<DefineRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        // Every domain method can name its verified caller; this one only logs it.
        let caller = gateway_identity(&context)?;
        tracing::debug!(subject = %caller.actor.subject, term = %request.term, "define");
        let definition =
            glossary::lookup(&request.term).ok_or_else(|| unknown_term(&request.term))?;
        structured_result(definition.title.clone(), &definition)
    }
}

impl Default for GlossaryMcp {
    fn default() -> Self {
        Self::new()
    }
}

impl DomainServer for GlossaryMcp {
    type Contract = GlossaryContract;

    fn setup() -> &'static McpServerSetup<GlossaryContract> {
        &SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    async fn read(
        &self,
        address: DomainAddress<GlossaryContract>,
        request: &ReadResourceRequestParams,
        _context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, McpError> {
        let result = match address {
            GlossaryResource::Terms => {
                let terms = glossary::entries()
                    .into_iter()
                    .map(|entry| {
                        let uri = GlossaryResource::Term(entry.term.clone())
                            .to_uri()
                            .map_err(|_| McpError::internal_error("term address", None))?;
                        Ok(TermSummary {
                            term: entry.term,
                            title: entry.title,
                            uri: uri.to_string(),
                        })
                    })
                    .collect::<Result<Vec<_>, McpError>>()?;
                json_read(&request.uri, &terms)?
            }
            GlossaryResource::Term(term) => json_read(
                &request.uri,
                &glossary::lookup(&term).ok_or_else(|| unknown_term(&term))?,
            )?,
            GlossaryResource::Docs | GlossaryResource::Document(_) | GlossaryResource::Contract => {
                return Err(served_by_host());
            }
        };
        Ok(DomainRead::private(result))
    }

    fn prompts(&self) -> Vec<Prompt> {
        vec![
            Prompt::new(
                EXPLAIN_PROMPT,
                Some("Explain one glossary term to a new contributor."),
                Some(vec![
                    PromptArgument::new("term")
                        .with_description("Term identifier, such as `hosted-server`.")
                        .with_required(true),
                ]),
            )
            .with_title("Explain term"),
        ]
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        #[derive(Deserialize)]
        struct ExplainArgs {
            term: TermId,
        }
        if request.name != EXPLAIN_PROMPT {
            return Err(unknown_prompt(&request.name));
        }
        let args: ExplainArgs = serde_json::from_value(serde_json::Value::Object(
            request.arguments.unwrap_or_default(),
        ))
        .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let uri = GlossaryResource::Term(args.term)
            .to_uri()
            .map_err(|_| McpError::internal_error("term address", None))?;
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Read {uri} and explain the term to a new contributor. Follow its \
                 related terms when they help."
            ),
        )]))
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        if reference.uri != TERM_TEMPLATE || request.argument.name != "term_id" {
            return Ok(CompleteResult::default());
        }
        let entries = glossary::entries();
        completion(rank_completions(
            entries.iter().map(|entry| entry.term.as_str()),
            &request.argument.value,
        ))
    }
}

fn unknown_term(term: &TermId) -> McpError {
    McpError::resource_not_found(
        format!("unknown term `{term}`; read glossary://terms"),
        None,
    )
}
