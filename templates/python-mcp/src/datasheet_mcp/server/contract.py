"""Datasheet's MCP associations; the SDK contains no Datasheet vocabulary."""
from __future__ import annotations

import mcp.types as types
from veoveo_mcp.contract.server import McpResource, McpResourceTemplate, McpServerContract, McpServerSetup
from veoveo_mcp.types import ResourceTemplateUri

from .. import uris
from ..docs import SERVER_DOCS

INSTRUCTIONS = (
    "Datasheet profiling server. Use direct tools for small previews and "
    "column statistics; run profile_dataset as an MCP task for the full "
    "profile and shared-plane artifact output. Resources expose reports, "
    "per-task usage, and artifacts under the datasheet:// scheme."
)


def _resource(address: uris.DatasheetResource, name: str, title: str,
              description: str, mime_type: str = "application/json") -> McpResource[uris.DatasheetResource]:
    return McpResource.build(address, lambda uri: types.Resource(
        uri=uri, name=name, title=title, description=description, mime_type=mime_type,
    ))


def _template(uri: ResourceTemplateUri, name: str, title: str, description: str,
              mime_type: str = "application/json") -> McpResourceTemplate:
    return McpResourceTemplate.build(uri, lambda value: types.ResourceTemplate(
        uri_template=value, name=name, title=title, description=description, mime_type=mime_type,
    ))


class DatasheetContract:
    name = "datasheet"
    version = "0.1.0"
    instructions = INSTRUCTIONS
    scheme = uris.SCHEME
    scope_type = uris.DatasheetScope
    scopes = tuple(uris.DatasheetScope)
    documents = SERVER_DOCS
    parse_resource = staticmethod(uris.parse_resource_uri)

    def resources(self) -> tuple[McpResource[uris.DatasheetResource], ...]:
        workbench = McpResource.build(uris.FixedResource.WORKBENCH, lambda uri: types.Resource(
            uri=uri, name="workbench", title="Workbench",
            description="Preview, inspect, and profile governed tabular data.",
            mime_type="text/html;profile=mcp-app", meta={"ui": {}},
        ))
        return (
            workbench,
            _resource(uris.ReportCatalogResource(), "reports", "Profile reports",
                      "Completed and running datasheet profile tasks."),
            _resource(uris.UsageCatalogResource(), "usage", "Datasheet usage ledger",
                      "Index of task usage resources."),
            _resource(uris.DocumentCatalogResource(), "docs", "Datasheet server documentation",
                      "Index of embedded server documents."),
            _resource(uris.FixedResource.CONTRACT, "contract", "Datasheet contract declaration",
                      "Machine-readable contract revision, compliance, and capability inventory."),
            *(_resource(uris.DocumentResource(uris.DocumentId(doc.id)), doc.id, doc.title,
                        f"Embedded `{doc.id}` server document.", "text/markdown")
              for doc in self.documents),
        )

    def resource_templates(self) -> tuple[McpResourceTemplate, ...]:
        return (
            _template(uris.DOCS_TEMPLATE, "documents", "Server documentation",
                      "Embedded server documents.", "text/markdown"),
            _template(uris.REPORTS_TEMPLATE, "report-pages", "Profile report pages",
                      "Read the nextUri returned by the report catalog."),
            _template(uris.USAGE_TEMPLATE, "usage-pages", "Usage catalog pages",
                      "Read the nextUri returned by the usage catalog."),
            _template(uris.USAGE_TASK_TEMPLATE, "usage", "Datasheet task usage",
                      "Usage rows for one datasheet task. task_id supports completion."),
            _template(uris.ARTIFACT_TEMPLATE, "artifact", "Datasheet artifact",
                      "Shared-plane immutable datasheet artifact."),
        )


CONTRACT: McpServerContract[uris.DatasheetScope, uris.DatasheetResource] = DatasheetContract()
SERVER_SETUP = McpServerSetup(CONTRACT)
