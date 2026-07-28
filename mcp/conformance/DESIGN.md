# MCP Conformance Design

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| Model Context Protocol | negotiated Streamable HTTP protocol plus the hosted-server requirements selected by a typed profile |
| JSON-RPC 2.0 | MCP request and response envelopes |
| JSON Schema 2020-12 | self-contained tool input schemas and generated profile/report schemas |
| OAuth 2.0 protected-resource metadata | unauthenticated Bearer rejection checks selected by the profile |
| `veoveo.io/mcp-conformance-profile/v1` | domain-neutral declaration of applicable hosted-server checks |
| `veoveo.io/mcp-conformance-report/v1` | machine-readable implementation identity, capabilities, requirement results, and evidence |
| `veoveo.io/hosted-mcp/v1` | initial Veoveo hosted-server contract revision |

## Boundary

This crate certifies a running MCP server through public HTTP and MCP surfaces. The
library accepts a typed profile and credentials supplied out of band, then returns a
typed report. The CLI reads and writes the same JSON contracts.

The crate depends on shared MCP protocol and Veoveo contract infrastructure. It does
not depend on a domain server, showcase, example, extension implementation, or client
repository. Domain lifecycle smoke remains with the component that owns the domain.

## Profile

A profile names the expected implementation slug, selected contract revision, allowed
resource URI schemes, HTTP boundary checks, and required, optional, or forbidden MCP
surfaces. Required tool, resource, template, and prompt identities are extension-owned
inputs rather than compiled registry entries.

Credentials never enter the profile or report. The CLI receives a bearer through
`MCP_BEARER_TOKEN`, or uses the existing direct-hosted internal assertion arguments
for installation-local acceptance.

## Report

Each result carries a stable requirement identifier, status, summary, and bounded
evidence. The report records the negotiated protocol, implementation identity,
advertised capabilities, selected contract revision, and execution interval. A failed
requirement produces a report and a non-zero CLI exit.

## CLI Output

Each CLI command reserves standard output for its requested result. Structured resources
therefore remain parseable even when the server emits notifications while the command is
running. Unsolicited progress, task-status, resource-update, and list-change notifications
are operator diagnostics on standard error.

## Distribution

The thin `certify` binary is copied into the digest-addressed
`veoveo/mcp-conformance` OCI image. The image contains no server implementation and
runs as uid 10001. Installation operators mirror it into their private registry or
offline bundle and execute it against extension endpoints.
