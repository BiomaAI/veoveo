"""Owner route types, template agreement, and rejected aliases at the MCP edge."""
import pytest
from veoveo_mcp.contract.artifacts import ArtifactId
from veoveo_mcp.types import ResourceUri
from veoveo_mcp.tasks import new_task_id
from datasheet_mcp import uris
from datasheet_mcp.server.contract import SERVER_SETUP


def test_entire_static_discovery_uses_the_owner_parser():
    assert SERVER_SETUP.scope_names == frozenset()
    for descriptor in SERVER_SETUP.resources():
        address = uris.parse_resource_uri(ResourceUri(descriptor.uri))
        assert address is not None
        assert address.to_uri() == descriptor.uri


def test_templates_expand_into_typed_owner_resources():
    task = new_task_id()
    artifact = ArtifactId(str(task))
    pairs = (
        (uris.USAGE_TASK_TEMPLATE.expand(task_id=str(task)), uris.TaskUsageResource(task)),
        (uris.ARTIFACT_TEMPLATE.expand(artifact_id=artifact), uris.ArtifactResource(artifact)),
        (uris.DOCS_TEMPLATE.expand(doc_id="design"), uris.DocumentResource(uris.DocumentId.DESIGN)),
        (uris.REPORTS_TEMPLATE.expand(), uris.ReportCatalogResource()),
        (uris.USAGE_TEMPLATE.expand(), uris.UsageCatalogResource()),
    )
    for uri, address in pairs:
        assert address.to_uri() == uri
        assert uris.parse_resource_uri(uri) == address
    assert uris.parse_resource_uri(uris.DocumentCatalogResource(uris.DocumentId.AGENTS).to_uri()).after == uris.DocumentId.AGENTS


@pytest.mark.parametrize("uri", [
    "datasheet://docs/%64esign", "datasheet://docs/design?", "datasheet://docs/unknown",
    "datasheet://docs/design/extra", "datasheet://docs/design%2Fextra", "datasheet://reports/",
    "datasheet://reports?", "datasheet://reports?cursor=x&cursor=y", "datasheet://usage/task/%zz",
    "datasheet://artifact/019FFDB2-0598-7476-96D3-F3D7B0769F9E",
    "datasheet://docs:80/design", "datasheet://docs/design#fragment",
])
def test_owner_rejects_unknown_or_noncanonical_resource_spellings(uri):
    try:
        assert uris.parse_resource_uri(ResourceUri(uri)) is None
    except ValueError:
        pass


def test_builders_reject_wrong_identity_and_cursor_types():
    for construct, value in (
        (uris.ArtifactResource, str(new_task_id())),
        (uris.TaskUsageResource, str(new_task_id())),
        (uris.DocumentResource, "design"),
        (uris.DocumentCatalogResource, "design"),
        (uris.ReportCatalogResource, uris.UsageCursor(task_id=new_task_id())),
        (uris.UsageCatalogResource, uris.ReportCursor(task_id=new_task_id(), created_at="2026-10-02T00:00:00Z")),
    ):
        with pytest.raises(TypeError):
            construct(value)
