//! Checked MCP declarations; domain address types remain in the contract feature.
use rmcp::model::{Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use std::sync::LazyLock;
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_optimization_mcp::{
    contract::{
        OptimizationCollection, OptimizationCollectionUri, OptimizationDocument,
        OptimizationResource, OptimizationScope, OptimizationTaskUsageUri,
        OptimizationUsageIndexUri, uris,
    },
    profiles::profiles,
};
use veoveo_types::{ResourceScheme, ResourceTemplateUri};

pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("optimization"));
pub(super) struct OptimizationContract;
pub(super) static SERVER_SETUP: LazyLock<McpServerSetup<OptimizationContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("Optimization MCP contract setup"));

impl McpServerContract for OptimizationContract {
    type Scope = OptimizationScope;
    type Resource = OptimizationResource;
    fn slug() -> ServerSlug {
        ServerSlug::new("optimization").expect("declared slug")
    }
    fn scheme() -> ResourceScheme {
        uris::SCHEME.clone()
    }
    fn scopes() -> &'static [OptimizationScope] {
        &[]
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        server_info()
    }
    fn resources() -> Result<Vec<McpResource<OptimizationResource>>, McpSetupError> {
        resources()
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        resource_templates()
    }
}

fn json_descriptor(
    address: OptimizationResource,
    title: &str,
    description: &str,
) -> Result<McpResource<OptimizationResource>, McpSetupError> {
    McpResource::new(address, |uri| {
        Resource::new(uri, title)
            .with_title(title)
            .with_description(description)
            .with_mime_type("application/json")
    })
}

fn resources() -> Result<Vec<McpResource<OptimizationResource>>, McpSetupError> {
    use OptimizationResource as R;
    let mut resources = vec![
        json_descriptor(
            R::Docs,
            "Server documents",
            "Index of crate documents embedded at build time.",
        )?,
        json_descriptor(
            R::Contract,
            "Contract declaration",
            "Machine-readable contract revision, compliance, and capabilities.",
        )?,
        json_descriptor(
            R::Capabilities,
            "Optimization capabilities",
            "Live cuOpt, GPU, contract, size-limit, and problem-family capabilities.",
        )?,
    ];
    for doc in SERVER_DOCS.iter() {
        resources.push(McpResource::new(
            R::Document(OptimizationDocument::parse(doc.id).expect("declared document")),
            |uri| {
                Resource::new(uri, doc.title)
                    .with_title(doc.title)
                    .with_description("Crate document embedded at build time.")
                    .with_mime_type("text/markdown")
            },
        )?);
    }
    for (address, name, title, description) in [
        (
            R::RoutesApp,
            "routes",
            "Routes",
            "GPU routing problems, runs, solutions, and verification.",
        ),
        (
            R::ModelsApp,
            "models",
            "Models",
            "GPU convex and mixed-integer models, runs, and solutions.",
        ),
    ] {
        resources.push(McpResource::new(address, |uri| {
            veoveo_mcp_apps_extension::app_resource(uri, name)
                .with_title(title)
                .with_description(description)
        })?);
    }
    for (address, title) in [
        (R::Profiles, "Solver profiles"),
        (
            R::Collection(
                OptimizationCollectionUri::new(OptimizationCollection::Problems, None)
                    .expect("declared root"),
            ),
            "Optimization problems",
        ),
        (
            R::Collection(
                OptimizationCollectionUri::new(OptimizationCollection::Runs, None)
                    .expect("declared root"),
            ),
            "Optimization runs",
        ),
        (
            R::Collection(
                OptimizationCollectionUri::new(OptimizationCollection::Solutions, None)
                    .expect("declared root"),
            ),
            "Optimization solutions",
        ),
        (
            R::Usage(OptimizationUsageIndexUri::new(None)),
            "Optimization usage",
        ),
    ] {
        resources.push(json_descriptor(
            address,
            title,
            "Authorized Optimization index.",
        )?);
    }
    for profile in profiles() {
        resources.push(json_descriptor(
            R::Profile(profile.profile_uri.clone()),
            &profile.title,
            &profile.description,
        )?);
    }
    Ok(resources)
}

fn server_info() -> ServerConfig {
    let mut capabilities = ServerCapabilities::builder()
        .enable_tools()
        .enable_prompts()
        .enable_resources()
        .enable_resources_subscribe()
        .enable_resources_list_changed()
        .enable_completions()
        .build();
    veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
    capabilities.extensions.get_or_insert_default().insert(
        rmcp::model::TASKS_EXTENSION_ID.to_owned(),
        rmcp::model::JsonObject::new(),
    );
    let mut info = ServerConfig::default();
    info.capabilities = capabilities;
    info.server_info = rmcp::model::Implementation::new("optimization", env!("CARGO_PKG_VERSION"));
    info.instructions = Some(
        "GPU optimization through NVIDIA cuOpt. Read optimization://capabilities and \
             optimization://profiles first. Use optimize_routes for one routing model, \
             optimize_route_scenarios for homogeneous alternatives, solve_convex for continuous \
             LP/QP/QCQP/SOCP models, and solve_milp for mixed-integer linear models. Every tool \
             uses the MCP Task API. Problem, run, and solution identities are separate immutable \
             resources. Inspect the solution verification resource before operational use."
            .to_owned(),
    );
    info
}

fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
    [
        (
            uris::DOC_TEMPLATE,
            "Server document",
            "Embedded crate document.",
        ),
        (
            uris::PROFILE_TEMPLATE,
            "Solver profile",
            "Curated bounded cuOpt solver policy.",
        ),
        (
            uris::PROBLEM_TEMPLATE,
            "Optimization problem",
            "Immutable normalized problem and dimensions.",
        ),
        (
            uris::PROBLEMS_PAGE_TEMPLATE,
            "Optimization problem page",
            "Bounded problem index page selected by its opaque cursor.",
        ),
        (
            uris::RUN_TEMPLATE,
            "Optimization run",
            "Durable execution state and engine provenance.",
        ),
        (
            uris::RUNS_PAGE_TEMPLATE,
            "Optimization run page",
            "Bounded run index page selected by its opaque cursor.",
        ),
        (
            uris::RUN_INCUMBENTS_TEMPLATE,
            "MILP incumbents",
            "Ordered retained incumbent summaries.",
        ),
        (
            uris::SOLUTION_TEMPLATE,
            "Optimization solution",
            "Immutable verified solution.",
        ),
        (
            uris::SOLUTIONS_PAGE_TEMPLATE,
            "Optimization solution page",
            "Bounded solution index page selected by its opaque cursor.",
        ),
        (
            uris::SOLUTION_ROUTES_TEMPLATE,
            "Solution routes",
            "Case-addressed vehicle routes.",
        ),
        (
            uris::SOLUTION_VARIABLES_TEMPLATE,
            "Solution variables",
            "Mathematical variable values.",
        ),
        (
            uris::SOLUTION_VERIFICATION_TEMPLATE,
            "Solution verification",
            "Independent feasibility and objective checks.",
        ),
        (
            uris::ARTIFACT_TEMPLATE,
            "Optimization artifact",
            "Immutable bytes on the shared artifact plane.",
        ),
        (
            OptimizationTaskUsageUri::TEMPLATE,
            "Optimization task usage",
            "Measured GPU solve usage.",
        ),
        (
            OptimizationUsageIndexUri::TEMPLATE,
            "Optimization usage page",
            "Bounded usage index page selected by its opaque cursor.",
        ),
    ]
    .into_iter()
    .map(|(uri, title, description)| {
        let template = ResourceTemplateUri::new(uri).map_err(|_| McpSetupError::InvalidTemplate)?;
        McpResourceTemplate::new(template, |uri| {
            ResourceTemplate::new(uri, title)
                .with_title(title)
                .with_description(description)
                .with_mime_type(if uri == uris::DOC_TEMPLATE {
                    "text/markdown"
                } else {
                    "application/json"
                })
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn checked_setup_preserves_the_fixed_public_catalog() {
        let setup = &*SERVER_SETUP;
        let mut expected = [
            "optimization://docs",
            "optimization://docs/agents",
            "optimization://docs/design",
            "optimization://contract",
            "optimization://capabilities",
            "optimization://profiles",
            "optimization://problems",
            "optimization://runs",
            "optimization://solutions",
            "optimization://usage",
            "ui://optimization/routes.html",
            "ui://optimization/models.html",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
        expected.extend(
            profiles()
                .iter()
                .map(|profile| profile.profile_uri.to_string()),
        );
        let actual = setup
            .resources()
            .iter()
            .map(|resource| resource.descriptor().uri.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected);
        assert_eq!(setup.resource_templates().len(), 15);
        assert!(setup.scope_names().is_empty());
        assert_eq!(setup.server_config().server_info.name, "optimization");
        assert!(
            setup
                .server_config()
                .capabilities
                .extensions
                .as_ref()
                .unwrap()
                .contains_key(rmcp::model::TASKS_EXTENSION_ID)
        );
        for template in setup.resource_templates() {
            assert_eq!(
                template.descriptor().mime_type.as_deref(),
                Some(if template.template().as_str() == uris::DOC_TEMPLATE {
                    "text/markdown"
                } else {
                    "application/json"
                })
            );
        }
    }

    #[test]
    fn checked_setup_preserves_app_extension_metadata() {
        for (uri, name, title, description) in [
            (
                uris::ROUTES_APP_URI,
                "routes",
                "Routes",
                "GPU routing problems, runs, solutions, and verification.",
            ),
            (
                uris::MODELS_APP_URI,
                "models",
                "Models",
                "GPU convex and mixed-integer models, runs, and solutions.",
            ),
        ] {
            let actual = SERVER_SETUP
                .resources()
                .iter()
                .find(|resource| resource.descriptor().uri == uri)
                .unwrap()
                .descriptor();
            let expected = veoveo_mcp_apps_extension::app_resource(uri, name)
                .with_title(title)
                .with_description(description);
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
        }
    }
}
