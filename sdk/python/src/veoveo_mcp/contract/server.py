"""Checked MCP discovery assembled from an independently owned server contract."""
from __future__ import annotations

from dataclasses import dataclass
from collections.abc import Set
from typing import Callable, Generic, Protocol, TypeVar

import mcp.types as types
from mcp.server import Server

from veoveo_mcp.types import (
    ResourceAddress, ResourceScheme, ResourceTemplateUri, ResourceUri,
    ResourceUriBuilder, ScopeDefinition, ScopeName, UriAuthority, UriSegment,
)
from .docs import DOC_ID_AGENTS, DOC_ID_DESIGN, ContractDeclaration, ServerDocs
from .knowledge import EXTENSION_ID

A = TypeVar("A", bound=ResourceAddress)
S = TypeVar("S", bound=ScopeDefinition)


@dataclass(frozen=True)
class McpResource(Generic[A]):
    address: A
    descriptor: types.Resource

    @classmethod
    def build(cls, address: A, describe: Callable[[ResourceUri], types.Resource]) -> McpResource[A]:
        uri = address.to_uri()
        if not isinstance(uri, ResourceUri):
            raise TypeError("resource builders must return ResourceUri")
        descriptor = describe(uri)
        if descriptor.uri != uri or not descriptor.name.strip():
            raise ValueError("resource descriptor must preserve its address and have a name")
        return cls(address, descriptor.model_copy(deep=True))


@dataclass(frozen=True)
class McpResourceTemplate:
    template: ResourceTemplateUri
    descriptor: types.ResourceTemplate

    @classmethod
    def build(cls, template: ResourceTemplateUri,
              describe: Callable[[ResourceTemplateUri], types.ResourceTemplate]) -> McpResourceTemplate:
        if not isinstance(template, ResourceTemplateUri):
            raise TypeError("template builders require ResourceTemplateUri")
        descriptor = describe(template)
        if descriptor.uri_template != template or not descriptor.name.strip():
            raise ValueError("template descriptor must preserve its address and have a name")
        return cls(template, descriptor.model_copy(deep=True))


class McpServerContract(Protocol[S, A]):
    """Owner association; implementing this protocol does not prove compliance."""

    name: str
    version: str
    instructions: str
    scheme: ResourceScheme
    scope_type: type[S]
    scopes: tuple[S, ...]
    documents: ServerDocs

    def parse_resource(self, uri: ResourceUri) -> A | None: ...
    def resources(self) -> tuple[McpResource[A], ...]: ...
    def resource_templates(self) -> tuple[McpResourceTemplate, ...]: ...


@dataclass(frozen=True, init=False)
class McpServerSetup(Generic[S, A]):
    """Validate before dependency startup; handlers consume private descriptor copies."""

    name: str
    version: str
    instructions: str
    documents: ServerDocs
    scope_names: frozenset[ScopeName]
    _scope_type: type[S]
    _resources: tuple[types.Resource, ...]
    _templates: tuple[types.ResourceTemplate, ...]

    def __init__(self, contract: McpServerContract[S, A]) -> None:
        if contract.documents.server != contract.name or not contract.name.strip():
            raise ValueError("server identity and document identity must agree")
        if not contract.version.strip():
            raise ValueError("server version is required")
        if not isinstance(contract.scheme, ResourceScheme):
            raise TypeError("server contracts require a checked ResourceScheme")
        scopes: set[ScopeName] = set()
        for scope in contract.scopes:
            if not isinstance(scope, contract.scope_type):
                raise TypeError("scope declarations must use the owner's scope type")
            name = scope.scope_name()
            if not isinstance(name, ScopeName):
                raise TypeError("scope definitions must return ScopeName")
            if name in scopes:
                raise ValueError("duplicate scope declaration")
            scopes.add(name)

        resources: dict[ResourceUri, types.Resource] = {}
        for resource in contract.resources():
            uri = resource.address.to_uri()
            if not isinstance(uri, ResourceUri):
                raise TypeError("resource addresses must return ResourceUri")
            uri.components()
            if contract.parse_resource(uri) != resource.address:
                raise ValueError("resource address does not round trip through its owner parser")
            if resource.descriptor.uri != uri or not resource.descriptor.name.strip():
                raise ValueError("invalid resource descriptor")
            if uri in resources:
                raise ValueError("duplicate resource declaration")
            resources[uri] = resource.descriptor.model_copy(deep=True)

        def well_known(root: str, document: str | None = None) -> ResourceUri:
            builder = ResourceUriBuilder(contract.scheme, UriAuthority(root))
            if document is not None:
                builder = builder.segment(UriSegment(document))
            return builder.build()

        if not {well_known("docs"), well_known("contract")} <= resources.keys():
            raise ValueError("server must declare docs and contract resources")
        documents = contract.documents
        ContractDeclaration.from_docs(documents)
        if any(documents.doc(name) is None for name in (DOC_ID_AGENTS, DOC_ID_DESIGN)):
            raise ValueError("server must embed agents and design documents")
        document_ids: set[str] = set()
        for document in documents:
            if not document.body.strip() or document.id in document_ids:
                raise ValueError("server documents must be nonempty and unique")
            document_ids.add(document.id)
            if well_known("docs", document.id) not in resources:
                raise ValueError("embedded document is missing its resource declaration")

        docs_template = documents.knowledge_template(contract.scheme)
        templates: dict[ResourceTemplateUri, types.ResourceTemplate] = {}
        for entry in contract.resource_templates():
            if not isinstance(entry.template, ResourceTemplateUri):
                raise TypeError("resource templates require ResourceTemplateUri")
            descriptor = entry.descriptor.model_copy(deep=True)
            if descriptor.uri_template != entry.template or not descriptor.name.strip():
                raise ValueError("invalid resource template descriptor")
            if entry.template in templates:
                raise ValueError("duplicate resource template declaration")
            if entry.template == docs_template.uri_template:
                descriptor.meta = {**(descriptor.meta or {}), **(docs_template.meta or {})}
            templates[entry.template] = descriptor
        if docs_template.uri_template not in templates:
            raise ValueError("server must declare the shared document template")

        object.__setattr__(self, "name", contract.name)
        object.__setattr__(self, "version", contract.version)
        object.__setattr__(self, "instructions", contract.instructions)
        object.__setattr__(self, "documents", documents)
        object.__setattr__(self, "scope_names", frozenset(scopes))
        object.__setattr__(self, "_scope_type", contract.scope_type)
        object.__setattr__(self, "_resources", tuple(resources[uri] for uri in sorted(resources)))
        object.__setattr__(self, "_templates", tuple(templates[uri] for uri in sorted(templates)))

    def resources(self) -> list[types.Resource]:
        return [descriptor.model_copy(deep=True) for descriptor in self._resources]

    def resource_templates(self) -> list[types.ResourceTemplate]:
        return [descriptor.model_copy(deep=True) for descriptor in self._templates]

    def has_scope(self, grants: Set[ScopeName], required: S) -> bool:
        """Compare an owner permission with already authenticated grants."""
        if not isinstance(required, self._scope_type):
            raise TypeError("permission checks require the owner's scope type")
        name = required.scope_name()
        return name in self.scope_names and name in grants

    def configure(self, server: Server) -> None:
        if server.name != self.name or server.version != self.version:
            raise ValueError("MCP server identity differs from its checked setup")
        if any(server.get_request_handler(method) is None for method in (
                "resources/list", "resources/templates/list", "resources/read")):
            raise ValueError("checked setup requires resource list, template and read handlers")
        server.extensions[EXTENSION_ID] = {}
